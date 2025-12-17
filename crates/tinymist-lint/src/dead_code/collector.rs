//! Definition collector for dead code analysis.

use rustc_hash::FxHashSet;
use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{ArgExpr, Decl, DefKind, Expr, ExprInfo, Pattern, PatternSig},
};
use typst::syntax::{FileId, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefScope {
    /// File-level private definition (not exported).
    File,
    /// Module-level exported definition.
    Exported,
    /// Function parameter.
    FunctionParam,
    /// Block-level local definition.
    Local,
}

#[derive(Debug, Clone)]
pub struct DefInfo {
    /// The declaration expression.
    pub decl: Interned<Decl>,
    /// The kind of definition.
    pub kind: DefKind,
    /// The scope level.
    pub scope: DefScope,
    /// The file ID where this definition appears.
    #[allow(dead_code)]
    pub fid: FileId,
    /// The span of the declaration.
    pub span: Span,
}

pub fn collect_definitions(ei: &ExprInfo) -> Vec<DefInfo> {
    let mut definitions = Vec::new();
    let mut collector = DefinitionCollector {
        ei,
        definitions: &mut definitions,
        fid: ei.fid,
        ignored: FxHashSet::default(),
    };

    collector.collect_exports();
    collector.collect_resolves();
    collector.visit_expr(&ei.root, DefScope::File);

    collector
        .definitions
        .retain(|def| !collector.ignored.contains(&def.decl));

    definitions
}

struct DefinitionCollector<'a> {
    ei: &'a ExprInfo,
    definitions: &'a mut Vec<DefInfo>,
    fid: FileId,
    ignored: FxHashSet<Interned<Decl>>,
}

impl<'a> DefinitionCollector<'a> {
    fn collect_exports(&mut self) {
        for (_name, expr) in self.ei.exports.iter() {
            if let Some(decl) = Self::extract_decl(expr) {
                if decl.is_def() {
                    self.add_definition(decl, DefScope::Exported);
                }
            }
        }
    }

    fn collect_resolves(&mut self) {
        for (_span, ref_expr) in self.ei.resolves.iter() {
            let decl = &ref_expr.decl;
            if matches!(decl.as_ref(), Decl::Import(_) | Decl::ImportAlias(_)) {
                self.add_definition(decl.clone(), DefScope::File);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr, scope: DefScope) {
        match expr {
            Expr::Block(exprs) => {
                for e in exprs.iter() {
                    self.visit_expr(e, scope);
                }
            }

            Expr::Let(let_expr) => {
                self.collect_pattern(&let_expr.pattern, scope);
                if let Some(ref body) = let_expr.body {
                    self.visit_expr(body, DefScope::Local);
                }
            }

            Expr::Func(func_expr) => {
                self.add_definition(func_expr.decl.clone(), scope);
                self.collect_pattern_sig(&func_expr.params, DefScope::FunctionParam);
                self.visit_expr(&func_expr.body, DefScope::Local);
            }

            Expr::Show(show_expr) => {
                if let Some(ref selector) = show_expr.selector {
                    self.visit_expr(selector, scope);
                }
                self.visit_expr(&show_expr.edit, scope);
            }

            Expr::Set(set_expr) => {
                self.visit_expr(&set_expr.target, scope);
                self.visit_expr(&set_expr.args, scope);
                if let Some(ref cond) = set_expr.cond {
                    self.visit_expr(cond, scope);
                }
            }

            Expr::Conditional(if_expr) => {
                self.visit_expr(&if_expr.cond, scope);
                self.visit_expr(&if_expr.then, scope);
                self.visit_expr(&if_expr.else_, scope);
            }

            Expr::WhileLoop(while_expr) => {
                self.visit_expr(&while_expr.cond, scope);
                self.visit_expr(&while_expr.body, DefScope::Local);
            }

            Expr::ForLoop(for_expr) => {
                self.visit_expr(&for_expr.iter, scope);
                self.collect_pattern(&for_expr.pattern, DefScope::Local);
                self.visit_expr(&for_expr.body, DefScope::Local);
            }

            Expr::Array(args) | Expr::Dict(args) | Expr::Args(args) => {
                for arg in &args.args {
                    match arg {
                        ArgExpr::Pos(e) => {
                            self.visit_expr(e, scope);
                        }
                        ArgExpr::Named(named) => {
                            self.visit_expr(&named.1, scope);
                        }
                        ArgExpr::NamedRt(named_rt) => {
                            let key = &named_rt.0;
                            let val = &named_rt.1;
                            self.visit_expr(key, scope);
                            self.visit_expr(val, scope);
                        }
                        ArgExpr::Spread(e) => {
                            self.visit_expr(e, scope);
                        }
                    }
                }
            }

            Expr::Apply(apply) => {
                self.visit_expr(&apply.callee, scope);
                self.visit_expr(&apply.args, scope);
            }

            Expr::Binary(bin) => {
                self.visit_expr(&bin.operands.0, scope);
                self.visit_expr(&bin.operands.1, scope);
            }

            Expr::Unary(un) => {
                self.visit_expr(&un.lhs, scope);
            }

            Expr::Select(select) => {
                self.visit_expr(&select.lhs, scope);
            }

            Expr::Import(import) => {
                self.add_definition(import.decl.decl.clone(), DefScope::File);
            }

            Expr::Include(include) => {
                self.visit_expr(&include.source, scope);
            }

            Expr::Element(elem) => {
                for content in &elem.content {
                    self.visit_expr(content, scope);
                }
            }

            Expr::Contextual(e) => {
                self.visit_expr(e, scope);
            }

            Expr::Pattern(pat) => {
                self.collect_pattern(pat, scope);
            }

            Expr::Decl(_) | Expr::Ref(_) | Expr::ContentRef(_) | Expr::Type(_) | Expr::Star => {}
        }
    }

    fn collect_pattern(&mut self, pattern: &Interned<Pattern>, scope: DefScope) {
        match pattern.as_ref() {
            Pattern::Simple(decl) => {
                if decl.is_def() {
                    self.add_definition(decl.clone(), scope);
                }
            }
            Pattern::Sig(sig) => {
                self.collect_pattern_sig(sig.as_ref(), scope);
            }
            Pattern::Expr(expr) => {
                self.visit_expr(expr, scope);
            }
        }
    }

    fn collect_pattern_sig(&mut self, sig: &PatternSig, scope: DefScope) {
        for param in &sig.pos {
            self.collect_pattern(param, scope);
        }

        let named_are_bindings = matches!(scope, DefScope::FunctionParam);
        for (decl, default) in &sig.named {
            if named_are_bindings {
                if decl.is_def() {
                    self.add_definition(decl.clone(), scope);
                }
            } else {
                self.ignored.insert(decl.clone());
            }
            self.collect_pattern(default, scope);
        }

        if let Some((decl, pattern)) = &sig.spread_left {
            if decl.is_def() {
                self.add_definition(decl.clone(), scope);
            }
            self.collect_pattern(pattern, scope);
        }

        if let Some((decl, pattern)) = &sig.spread_right {
            if decl.is_def() {
                self.add_definition(decl.clone(), scope);
            }
            self.collect_pattern(pattern, scope);
        }
    }

    fn add_definition(&mut self, decl: Interned<Decl>, scope: DefScope) {
        match decl.as_ref() {
            Decl::IdentRef(_) | Decl::ContentRef(_) => {
                return;
            }
            _ => {}
        }
        if self.ignored.contains(&decl) {
            return;
        }
        let kind = decl.kind();
        let span = decl.span();

        self.definitions.push(DefInfo {
            decl,
            kind,
            scope,
            fid: self.fid,
            span,
        });
    }

    fn extract_decl(expr: &Expr) -> Option<Interned<Decl>> {
        match expr {
            Expr::Decl(decl) => Some(decl.clone()),
            Expr::Ref(ref_expr) => Some(ref_expr.decl.clone()),
            _ => None,
        }
    }
}
