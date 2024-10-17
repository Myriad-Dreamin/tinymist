use core::fmt;
use std::{collections::BTreeMap, path::Path};

use typst::{
    foundations::{Element, Func, Module, Type, Value},
    model::{EmphElem, EnumElem, HeadingElem, ListElem, StrongElem},
    syntax::{Span, SyntaxNode},
    Library,
};

use crate::{
    adt::interner::impl_internable,
    prelude::*,
    ty::{BuiltinTy, InsTy, Interned, SelectTy, Ty},
};

use super::InterpretMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A sequence of expressions
    Seq(Interned<EcoVec<Expr>>),
    /// A array literal
    Array(Interned<EcoVec<Expr>>),
    /// A dict literal
    Dict(Interned<EcoVec<Expr>>),
    /// An args literal
    Args(Interned<EcoVec<Expr>>),
    /// An args literal
    Pattern(Interned<Pattern>),
    /// A element literal
    Element(Interned<ElementExpr>),
    /// A unary operation
    Unary(Interned<UnExpr>),
    /// A binary operation
    Binary(Interned<BinExpr>),
    /// A function call
    Apply(Interned<ApplyExpr>),
    /// A function
    Func(Interned<FuncExpr>),
    /// A let
    Let(Interned<LetExpr>),
    Show(Interned<ShowExpr>),
    Set(Interned<SetExpr>),
    Ref(Interned<RefExpr>),
    ContentRef(Interned<ContentRefExpr>),
    Select(Interned<SelectExpr>),
    Deselect(Interned<SelectExpr>),
    Destruct(Interned<DestructExpr>),
    Import(Interned<ImportExpr>),
    Include(Interned<IncludeExpr>),
    Contextual(Interned<Expr>),
    Conditional(Interned<IfExpr>),
    WhileLoop(Interned<WhileExpr>),
    ForLoop(Interned<ForExpr>),
    Type(Ty),
    Decl(Interned<Decl>),
    Star,
    Def(DefKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DefKind {
    Var,
    Module,
    Func,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Decl {
    Ident(Interned<DeclIdent>),
    Label(Interned<DeclIdent>),
    ModuleImport(Span),
    Closure(Span),
    Spread(Span),
}

impl From<Decl> for Expr {
    fn from(decl: Decl) -> Self {
        Expr::Decl(decl.into())
    }
}

impl fmt::Display for Decl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Decl::Ident(ident) => write!(f, "Ident({:?})", ident.name),
            Decl::Label(ident) => write!(f, "Label({:?})", ident.name),
            Decl::ModuleImport(..) => write!(f, "ModuleImport(..)"),
            Decl::Closure(..) => write!(f, "Closure(..)"),
            Decl::Spread(..) => write!(f, "Spread(..)"),
        }
    }
}

impl Decl {
    pub fn name(&self) -> &Interned<str> {
        match self {
            Decl::Ident(ident) | Decl::Label(ident) => &ident.name,
            Decl::ModuleImport(_) | Decl::Closure(_) | Decl::Spread(_) => Interned::empty(),
        }
    }

    pub fn file_id(&self) -> Option<TypstFileId> {
        if let Some(s) = self.span() {
            return s.id();
        }
        match self {
            Decl::Ident(ident) | Decl::Label(ident) => match &ident.at {
                IdentAt::Export(fid) => Some(*fid),
                _ => None,
            },
            Decl::ModuleImport(..) | Decl::Closure(..) | Decl::Spread(..) => None,
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Decl::Ident(ident) | Decl::Label(ident) => Some(match &ident.at {
                IdentAt::Label(s) | IdentAt::Ref(s) | IdentAt::Span(s) => *s,
                IdentAt::Str(s) | IdentAt::PathStem(s) => s.span(),
                IdentAt::Export(..) => return None,
            }),
            Decl::ModuleImport(s) | Decl::Closure(s) | Decl::Spread(s) => Some(*s),
        }
    }

    fn as_def(&self, def: DefKind, val: Option<Ty>) -> Interned<RefExpr> {
        let exp = RefExpr {
            ident: self.clone(),
            of: Some(Expr::Def(def)),
            val,
        };
        Interned::new(exp)
    }
}

type UnExpr = UnInst<Expr>;
type BinExpr = BinInst<Expr>;

#[derive(Debug)]
pub struct ExprInfo {
    pub resolves: HashMap<Span, Interned<RefExpr>>,
    pub exports: BTreeMap<Interned<str>, Expr>,
    pub root: Expr,
}

impl std::hash::Hash for ExprInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.root.hash(state);
    }
}

pub(crate) fn expr_of(ctx: &AnalysisContext, source: Source) -> Arc<ExprInfo> {
    let mut w = ExprWorker {
        ctx,
        library: &ctx.world().library,
        mode: InterpretMode::Markup,
        scopes: vec![],
        info: ExprInfo {
            resolves: HashMap::default(),
            exports: BTreeMap::default(),
            root: none_expr(),
        },
    };
    let _ = w.scope_mut();
    let root = source.root().cast::<ast::Markup>().unwrap();
    let root = w.check_in_mode(root.exprs(), InterpretMode::Markup);
    w.info.root = root;
    w.info.exports = w.summarize_scope();
    Arc::new(w.info)
}

type LexicalScope = BTreeMap<Interned<str>, Expr>;

#[derive(Debug)]
enum ExprScope {
    Lexical(LexicalScope),
    Module(Module),
    Func(Func),
    Type(Type),
}

struct ExprWorker<'a, 'w> {
    ctx: &'a AnalysisContext<'w>,
    library: &'w Library,
    mode: InterpretMode,
    scopes: Vec<ExprScope>,
    info: ExprInfo,
}

impl<'a, 'w> ExprWorker<'a, 'w> {
    fn with_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let len = self.scopes.len();
        self.scopes.push(ExprScope::Lexical(BTreeMap::new()));
        let result = f(self);
        self.scopes.truncate(len);
        result
    }

    #[must_use]
    fn scope_mut(&mut self) -> &mut LexicalScope {
        if matches!(self.scopes.last(), Some(ExprScope::Lexical(_))) {
            return self.lexical_scope_unchecked();
        }
        let scope = BTreeMap::new();
        self.scopes.push(ExprScope::Lexical(scope));
        self.lexical_scope_unchecked()
    }

    fn lexical_scope_unchecked(&mut self) -> &mut LexicalScope {
        let scope = self.scopes.last_mut().unwrap();
        if let ExprScope::Lexical(scope) = scope {
            scope
        } else {
            unreachable!()
        }
    }

    fn alloc_ident(ident: ast::Ident) -> Decl {
        let ident = DeclIdent {
            name: ident.get().into(),
            at: IdentAt::Span(ident.span()),
        };
        Decl::Ident(ident.into())
    }

    fn alloc_str_key(&self, s: SyntaxNode, name: &str) -> Decl {
        let ident = DeclIdent {
            name: name.into(),
            at: IdentAt::Str(Box::new(s)),
        };
        Decl::Ident(ident.into())
    }

    fn alloc_path_stem(&self, s: SyntaxNode, name: &str) -> Decl {
        let ident = DeclIdent {
            name: name.into(),
            at: IdentAt::PathStem(Box::new(s)),
        };
        Decl::Ident(ident.into())
    }

    fn alloc_external(fid: TypstFileId, name: Interned<str>) -> Decl {
        let ident = DeclIdent {
            name,
            at: IdentAt::Export(fid),
        };
        Decl::Ident(ident.into())
    }

    fn summarize_scope(&mut self) -> BTreeMap<Interned<str>, Expr> {
        log::debug!("summarize_scope: {:?}", self.scopes);
        let mut exports = BTreeMap::new();
        for scope in std::mem::take(&mut self.scopes).into_iter() {
            match scope {
                ExprScope::Lexical(mut scope) => {
                    exports.append(&mut scope);
                }
                ExprScope::Module(module) => {
                    log::debug!("imported: {module:?}");
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Module(module.clone()))));
                    for (name, _) in module.scope().iter() {
                        let name: Interned<str> = name.into();
                        exports.insert(name.clone(), select_of(v.clone(), name));
                    }
                }
                ExprScope::Func(func) => {
                    if let Some(scope) = func.scope() {
                        let v = Interned::new(Ty::Value(InsTy::new(Value::Func(func.clone()))));
                        for (name, _) in scope.iter() {
                            let name: Interned<str> = name.into();
                            exports.insert(name.clone(), select_of(v.clone(), name));
                        }
                    }
                }
                ExprScope::Type(ty) => {
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Type(ty))));
                    for (name, _) in ty.scope().iter() {
                        let name: Interned<str> = name.into();
                        exports.insert(name.clone(), select_of(v.clone(), name));
                    }
                }
            }
        }
        exports
    }

    fn check(&mut self, m: ast::Expr) -> Expr {
        use ast::Expr::*;
        match m {
            None(_) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            Auto(..) => Expr::Type(Ty::Builtin(BuiltinTy::Auto)),
            Bool(bool) => Expr::Type(Ty::Value(InsTy::new(Value::Bool(bool.get())))),
            Int(int) => Expr::Type(Ty::Value(InsTy::new(Value::Int(int.get())))),
            Float(float) => Expr::Type(Ty::Value(InsTy::new(Value::Float(float.get())))),
            Numeric(numeric) => Expr::Type(Ty::Value(InsTy::new(Value::numeric(numeric.get())))),
            Str(s) => Expr::Type(Ty::Value(InsTy::new(Value::Str(s.get().into())))),

            Equation(equation) => self.check_math(equation.body()),
            Math(math) => self.check_math(math),
            Code(code_block) => self.check_code(code_block.body()),
            Content(c) => self.check_markup(c.body()),

            Ident(ident) => self.check_ident(ident),
            MathIdent(math_ident) => self.check_math_ident(math_ident),
            Label(label) => self.check_label(label),
            Ref(r) => self.check_ref(r),

            Let(let_binding) => self.check_let(let_binding),
            Closure(closure) => self.check_closure(closure),
            Import(module_import) => self.check_module_import(module_import),
            Include(module_include) => self.check_module_include(module_include),

            Parenthesized(p) => self.check(p.expr()),
            Array(array) => self.check_array(array),
            Dict(dict) => self.check_dict(dict),
            Unary(unary) => self.check_unary(unary),
            Binary(binary) => self.check_binary(binary),
            FieldAccess(field_access) => self.check_field_access(field_access),
            FuncCall(func_call) => self.check_func_call(func_call),
            DestructAssign(destruct_assignment) => self.check_destruct_assign(destruct_assignment),
            Set(set_rule) => self.check_set(set_rule),
            Show(show_rule) => self.check_show(show_rule),
            Contextual(contextual) => {
                Expr::Unary(UnInst::new(UnaryOp::Context, self.check(contextual.body())))
            }
            Conditional(conditional) => self.check_conditional(conditional),
            While(while_loop) => self.check_while_loop(while_loop),
            For(for_loop) => self.check_for_loop(for_loop),
            Break(..) => Expr::Type(Ty::Builtin(BuiltinTy::Break)),
            Continue(..) => Expr::Type(Ty::Builtin(BuiltinTy::Continue)),
            Return(func_return) => Expr::Unary(UnInst::new(
                UnaryOp::Return,
                func_return
                    .body()
                    .map_or_else(none_expr, |body| self.check(body)),
            )),

            Text(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Raw(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Link(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Space(..) => Expr::Type(Ty::Builtin(BuiltinTy::Space)),
            Linebreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Parbreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Escape(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Shorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            SmartQuote(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Strong(e) => self.check_element(Element::of::<StrongElem>(), e.body().exprs()),
            Emph(e) => self.check_element(Element::of::<EmphElem>(), e.body().exprs()),
            Heading(e) => self.check_element(Element::of::<HeadingElem>(), e.body().exprs()),
            List(e) => self.check_element(Element::of::<ListElem>(), e.body().exprs()),
            Enum(e) => self.check_element(Element::of::<EnumElem>(), e.body().exprs()),
            Term(t) => self.check_element(
                Element::of::<EnumElem>(),
                t.term().exprs().chain(t.description().exprs()),
            ),

            MathAlignPoint(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            MathDelimited(math_delimited) => {
                self.check_in_mode(math_delimited.body().exprs(), InterpretMode::Math)
            }
            MathAttach(ma) => self.check_in_mode(
                [
                    ma.base(),
                    ma.bottom().unwrap_or_default(),
                    ma.top().unwrap_or_default(),
                ]
                .into_iter(),
                InterpretMode::Math,
            ),
            MathPrimes(..) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            MathFrac(mf) => {
                self.check_in_mode([mf.num(), mf.denom()].into_iter(), InterpretMode::Math)
            }
            MathRoot(mr) => self.check(mr.radicand()),
        }
    }

    fn check_label(&mut self, ident: ast::Label) -> Expr {
        let ident = DeclIdent {
            name: ident.get().into(),
            at: IdentAt::Label(ident.span()),
        };
        Expr::Decl(Decl::Label(ident.into()).into())
    }

    fn check_element<'b>(
        &mut self,
        elem: Element,
        root: impl Iterator<Item = ast::Expr<'b>>,
    ) -> Expr {
        let content = root.map(|b| self.check(b)).collect();
        Expr::Element(ElementExpr { elem, content }.into())
    }

    fn check_let(&mut self, typed: ast::LetBinding) -> Expr {
        match typed.kind() {
            ast::LetBindingKind::Closure(..) => {
                typed.init().map_or_else(none_expr, |expr| self.check(expr))
            }
            ast::LetBindingKind::Normal(p) => {
                let body = match typed.init() {
                    Some(expr) => self.check(expr),
                    None => Expr::Type(Ty::Builtin(BuiltinTy::None)),
                };

                let pattern = self.check_pattern(p);
                Expr::Let(LetExpr { pattern, body }.into())
            }
        }
    }

    fn check_closure(&mut self, typed: ast::Closure) -> Expr {
        let decl = match typed.name() {
            Some(name) => Self::alloc_ident(name),
            None => Decl::Closure(typed.span()),
        };
        self.resolve_as(decl.as_def(DefKind::Func, None));

        let (params, body) = self.with_scope(|this| {
            this.scope_mut()
                .insert(decl.name().clone(), decl.clone().into());
            let mut params = eco_vec![];
            for arg in typed.params().children() {
                match arg {
                    ast::Param::Pos(arg) => {
                        params.push(this.check_pattern(arg));
                    }
                    ast::Param::Named(arg) => {
                        let key = Self::alloc_ident(arg.name());
                        let val = this.check(arg.expr());
                        params.push(Expr::Deselect(SelectExpr::new(key.clone(), val)));

                        this.resolve_as(key.as_def(DefKind::Var, None));
                        this.scope_mut().insert(key.name().clone(), key.into());
                    }
                    ast::Param::Spread(s) => {
                        let spreaded = this.check(s.expr());
                        let inst = UnInst::new(UnaryOp::Spread, spreaded);
                        params.push(Expr::Unary(inst));

                        let decl = if let Some(ident) = s.sink_ident() {
                            Self::alloc_ident(ident)
                        } else {
                            Decl::Spread(s.span())
                        };

                        this.resolve_as(decl.as_def(DefKind::Var, None));
                        this.scope_mut().insert(decl.name().clone(), decl.into());
                    }
                }
            }

            (params, this.check(typed.body()))
        });

        self.scope_mut()
            .insert(decl.name().clone(), decl.clone().into());
        Expr::Func(FuncExpr { decl, params, body }.into())
    }

    fn check_pattern(&mut self, typed: ast::Pattern) -> Expr {
        match typed {
            ast::Pattern::Normal(expr) => self.check_pattern_expr(expr),
            ast::Pattern::Placeholder(..) => Expr::Star,
            ast::Pattern::Parenthesized(p) => self.check_pattern(p.pattern()),
            ast::Pattern::Destructuring(d) => {
                let mut inputs = eco_vec![];
                let mut names = eco_vec![];
                let mut spread_left = None;
                let mut spread_right = None;

                for item in d.items() {
                    match item {
                        ast::DestructuringItem::Pattern(p) => {
                            inputs.push(self.check_pattern(p));
                        }
                        ast::DestructuringItem::Named(n) => {
                            let key = Self::alloc_ident(n.name());
                            let val = self.check_pattern_expr(n.expr());
                            names.push((key, val));
                        }
                        ast::DestructuringItem::Spread(s) => {
                            if inputs.is_empty() {
                                spread_left = Some(self.check_pattern_expr(s.expr()));
                            } else {
                                spread_right = Some(self.check_pattern_expr(s.expr()));
                            }
                        }
                    }
                }

                let pattern = Pattern {
                    inputs,
                    names,
                    spread_left,
                    spread_right,
                };

                Expr::Pattern(pattern.into())
            }
        }
    }

    fn check_pattern_expr(&mut self, typed: ast::Expr) -> Expr {
        match typed {
            ast::Expr::Ident(ident) => {
                let decl = Self::alloc_ident(ident);
                self.resolve_as(decl.as_def(DefKind::Var, None));
                self.scope_mut()
                    .insert(decl.name().clone(), decl.clone().into());
                Expr::Decl(decl.into())
            }
            ast::Expr::Parenthesized(parenthesized) => self.check_pattern(parenthesized.pattern()),
            ast::Expr::Closure(c) => self.check_closure(c),
            _ => self.check(typed),
        }
    }

    fn check_module_import(&mut self, typed: ast::ModuleImport) -> Expr {
        let source = typed.source().to_untyped();
        log::debug!("checking import: {source:?}");

        let (src, val) = self.ctx.analyze_import2(source);

        let decl = match (typed.new_name(), src) {
            (Some(ident), _) => Some(Self::alloc_ident(ident)),
            (None, Some(Value::Str(i))) if typed.imports().is_none() => {
                let i = Path::new(i.as_str());
                let name = i.file_stem().and_then(|s| s.to_str());
                // Some(self.alloc_path_end(s))
                name.map(|name| self.alloc_path_stem(source.clone(), name))
            }
            _ => None,
        };
        if let Some(decl) = &decl {
            self.scope_mut()
                .insert(decl.name().clone(), decl.clone().into());
        }
        let decl = decl.unwrap_or_else(|| Decl::ModuleImport(typed.span()));
        self.resolve_as(decl.as_def(
            DefKind::Module,
            val.clone().map(|val| Ty::Value(InsTy::new(val))),
        ));

        let pattern;

        if let Some(imports) = typed.imports() {
            match imports {
                ast::Imports::Wildcard => {
                    log::debug!("checking wildcard: {val:?}");
                    match val {
                        Some(Value::Module(m)) => {
                            self.scopes.push(ExprScope::Module(m));
                        }
                        Some(Value::Func(f)) => {
                            if f.scope().is_some() {
                                self.scopes.push(ExprScope::Func(f));
                            }
                        }
                        Some(Value::Type(s)) => {
                            self.scopes.push(ExprScope::Type(s));
                        }
                        Some(_) => {}
                        None => {
                            log::warn!(
                                "cannot analyze import on: {typed:?}, in file {:?}",
                                typed.span().id()
                            );
                        }
                    }

                    pattern = Expr::Star;
                }
                ast::Imports::Items(items) => {
                    let mut imported = eco_vec![];
                    let module = Expr::Decl(decl.clone().into());

                    for item in items.iter() {
                        let (old, new, is_renamed) = match item {
                            ast::ImportItem::Simple(ident) => {
                                let old = Self::alloc_ident(ident);
                                (old.clone(), old, false)
                            }
                            ast::ImportItem::Renamed(renamed) => {
                                let old_name = Self::alloc_ident(renamed.original_name());
                                let new_name = Self::alloc_ident(renamed.new_name());
                                (old_name, new_name, true)
                            }
                        };
                        let old_select = SelectExpr::new(old.clone(), module.clone());
                        if is_renamed {
                            self.resolve_as(
                                RefExpr {
                                    ident: new.clone(),
                                    of: Some(Expr::Decl(old.clone().into())),
                                    val: None,
                                }
                                .into(),
                            );
                        }
                        self.resolve_as(
                            RefExpr {
                                ident: old.clone(),
                                of: Some(Expr::Select(old_select.clone())),
                                val: None,
                            }
                            .into(),
                        );
                        self.scope_mut()
                            .insert(new.name().clone(), Expr::Select(old_select));
                        imported.push((old, Expr::Decl(new.into())));
                    }

                    pattern = Expr::Pattern(
                        Pattern {
                            inputs: eco_vec![],
                            names: imported,
                            spread_left: None,
                            spread_right: None,
                        }
                        .into(),
                    );
                }
            }
        } else {
            pattern = none_expr();
        };

        Expr::Import(ImportExpr { decl, pattern }.into())
    }

    fn check_module_include(&mut self, typed: ast::ModuleInclude) -> Expr {
        let source = self.check(typed.source());
        Expr::Include(IncludeExpr { source }.into())
    }

    fn check_array(&mut self, typed: ast::Array) -> Expr {
        let mut items = eco_vec![];
        for item in typed.items() {
            match item {
                ast::ArrayItem::Pos(item) => {
                    items.push(self.check(item));
                }
                ast::ArrayItem::Spread(s) => {
                    let spreaded = self.check(s.expr());
                    let inst = UnInst::new(UnaryOp::Spread, spreaded);
                    items.push(Expr::Unary(inst));
                }
            }
        }

        Expr::Array(items.into())
    }

    fn check_dict(&mut self, typed: ast::Dict) -> Expr {
        let mut items = eco_vec![];
        for item in typed.items() {
            match item {
                ast::DictItem::Named(item) => {
                    let key = Self::alloc_ident(item.name());
                    let val = self.check(item.expr());
                    items.push(Expr::Deselect(SelectExpr::new(key, val)));
                }
                ast::DictItem::Keyed(item) => {
                    let key = item.key().to_untyped();
                    let analyzed = self.ctx.analyze_expr2(key);
                    let analyzed = analyzed.iter().find_map(|(s, _)| match s {
                        Value::Str(s) => Some(s),
                        _ => None,
                    });
                    let Some(analyzed) = analyzed else {
                        continue;
                    };
                    let key = self.alloc_str_key(key.clone(), analyzed);
                    let val = self.check(item.expr());
                    items.push(Expr::Deselect(SelectExpr::new(key, val)));
                }
                ast::DictItem::Spread(s) => {
                    let spreaded = self.check(s.expr());
                    let inst = UnInst::new(UnaryOp::Spread, spreaded);
                    items.push(Expr::Unary(inst));
                }
            }
        }

        Expr::Dict(items.into())
    }

    fn check_args(&mut self, typed: ast::Args) -> Expr {
        let mut args = eco_vec![];
        for arg in typed.items() {
            match arg {
                ast::Arg::Pos(arg) => {
                    args.push(self.check(arg));
                }
                ast::Arg::Named(arg) => {
                    let key = Self::alloc_ident(arg.name());
                    let val = self.check(arg.expr());
                    args.push(Expr::Deselect(SelectExpr::new(key, val)));
                }
                ast::Arg::Spread(s) => {
                    let spreaded = self.check(s.expr());
                    let inst = UnInst::new(UnaryOp::Spread, spreaded);
                    args.push(Expr::Unary(inst));
                }
            }
        }
        Expr::Args(args.into())
    }

    fn check_unary(&mut self, typed: ast::Unary) -> Expr {
        let op = match typed.op() {
            ast::UnOp::Pos => UnaryOp::Pos,
            ast::UnOp::Neg => UnaryOp::Neg,
            ast::UnOp::Not => UnaryOp::Not,
        };
        let lhs = self.check(typed.expr());
        Expr::Unary(UnInst::new(op, lhs))
    }

    fn check_binary(&mut self, typed: ast::Binary) -> Expr {
        let lhs = self.check(typed.lhs());
        let rhs = self.check(typed.rhs());
        Expr::Binary(BinInst::new(typed.op(), lhs, rhs))
    }

    fn check_destruct_assign(&mut self, typed: ast::DestructAssignment) -> Expr {
        let pat = self.check_pattern(typed.pattern());
        let val = self.check(typed.value());
        let inst = BinInst::new(ast::BinOp::Assign, pat, val);
        Expr::Binary(inst)
    }

    fn check_field_access(&mut self, typed: ast::FieldAccess) -> Expr {
        let lhs = self.check(typed.target());
        let rhs = Self::alloc_ident(typed.field());
        Expr::Select(SelectExpr { lhs, key: rhs }.into())
    }

    fn check_func_call(&mut self, typed: ast::FuncCall) -> Expr {
        let callee = self.check(typed.callee());
        let args = self.check_args(typed.args());
        Expr::Apply(ApplyExpr { callee, args }.into())
    }

    fn check_set(&mut self, typed: ast::SetRule) -> Expr {
        let target = self.check(typed.target());
        let args = self.check_args(typed.args());
        let cond = typed.condition().map(|c| self.check(c));
        Expr::Set(SetExpr { target, args, cond }.into())
    }

    fn check_show(&mut self, typed: ast::ShowRule) -> Expr {
        let selector = typed.selector().map(|s| self.check(s));
        let edit = self.check(typed.transform());
        Expr::Show(ShowExpr { selector, edit }.into())
    }

    fn check_conditional(&mut self, typed: ast::Conditional) -> Expr {
        let cond = self.check(typed.condition());
        let then = self.check(typed.if_body());
        let else_ = typed
            .else_body()
            .map_or_else(none_expr, |expr| self.check(expr));
        Expr::Conditional(IfExpr { cond, then, else_ }.into())
    }

    fn check_while_loop(&mut self, typed: ast::WhileLoop) -> Expr {
        let cond = self.check(typed.condition());
        let body = self.check(typed.body());
        Expr::WhileLoop(WhileExpr { cond, body }.into())
    }

    fn check_for_loop(&mut self, typed: ast::ForLoop) -> Expr {
        let pattern = self.check_pattern(typed.pattern());
        let iter = self.check(typed.iterable());
        let body = self.check(typed.body());
        Expr::ForLoop(
            ForExpr {
                pattern,
                iter,
                body,
            }
            .into(),
        )
    }

    fn check_markup(&mut self, m: ast::Markup) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Markup))
    }

    fn check_code(&mut self, m: ast::Code) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Code))
    }

    fn check_math(&mut self, m: ast::Math) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Math))
    }

    fn check_in_mode<'b>(
        &mut self,
        root: impl Iterator<Item = ast::Expr<'b>>,
        mode: InterpretMode,
    ) -> Expr {
        let old_mode = self.mode;
        self.mode = mode;
        let mut children = EcoVec::new();
        for child in root {
            children.push(self.check(child));
        }
        self.mode = old_mode;
        Expr::Seq(children.into())
    }

    fn check_ref(&mut self, r: ast::Ref) -> Expr {
        let ident = DeclIdent {
            name: r.target().into(),
            at: IdentAt::Ref(r.span()),
        };
        let body = r.supplement().map(|s| self.check(ast::Expr::Content(s)));
        let ref_expr = ContentRefExpr {
            ident: Decl::Label(ident.into()),
            of: None,
            body,
        };
        Expr::ContentRef(ref_expr.into())
    }

    fn check_ident(&mut self, ident: ast::Ident) -> Expr {
        let decl = Self::alloc_ident(ident);
        self.resolve_ident(decl, InterpretMode::Code)
    }

    fn check_math_ident(&mut self, ident: ast::MathIdent) -> Expr {
        let ident = DeclIdent {
            name: ident.get().into(),
            at: IdentAt::Span(ident.span()),
        };
        let decl = Decl::Ident(ident.into());
        self.resolve_ident(decl, InterpretMode::Code)
    }

    fn resolve_as(&mut self, r: Interned<RefExpr>) {
        let s = r.ident.span().unwrap();
        self.info.resolves.insert(s, r.clone());
    }

    fn resolve_ident(&mut self, decl: Decl, code: InterpretMode) -> Expr {
        let r: Interned<RefExpr> = self.resolve_ident_(decl, code).into();
        let s = r.ident.span().unwrap();
        self.info.resolves.insert(s, r.clone());
        Expr::Ref(r)
    }

    fn resolve_ident_(&mut self, decl: Decl, code: InterpretMode) -> RefExpr {
        let name = decl.name().clone();

        let mut ref_expr = RefExpr {
            ident: decl,
            of: None,
            val: None,
        };
        for scope in self.scopes.iter().rev() {
            let (of, val) = match scope {
                ExprScope::Lexical(scope) => {
                    if let Some(of) = scope.get(&name) {
                        (Some(of.clone()), None)
                    } else {
                        continue;
                    }
                }
                ExprScope::Module(module) => {
                    let v = module.scope().get(&name);

                    if let Some(fid) = module.file_id() {
                        (
                            Some(Expr::Decl(Self::alloc_external(fid, name.clone()).into())),
                            v,
                        )
                    } else {
                        (None, v)
                    }
                }
                ExprScope::Func(func) => {
                    let v = func.scope().unwrap().get(&name);
                    (None, v)
                }
                ExprScope::Type(ty) => {
                    let v = ty.scope().get(&name);
                    (None, v)
                }
            };

            ref_expr.of = of.clone();
            ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
            return ref_expr;
        }

        let scope = match code {
            InterpretMode::Math => self.library.math.scope(),
            InterpretMode::Markup | InterpretMode::Code => self.library.global.scope(),
            _ => return ref_expr,
        };

        let val = scope.get(&name);
        ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
        ref_expr
    }
}

fn select_of(source: Interned<Ty>, name: Interned<str>) -> Expr {
    Expr::Type(Ty::Select(SelectTy::new(source, name)))
}

fn none_expr() -> Expr {
    Expr::Type(Ty::Builtin(BuiltinTy::None))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern {
    inputs: EcoVec<Expr>,
    names: EcoVec<(Decl, Expr)>,
    spread_left: Option<Expr>,
    spread_right: Option<Expr>,
}

impl Pattern {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeclNameless {
    info: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeclIdent {
    name: Interned<str>,
    at: IdentAt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdentAt {
    Export(TypstFileId),
    Span(Span),
    Label(Span),
    Ref(Span),
    Str(Box<SyntaxNode>),
    PathStem(Box<SyntaxNode>),
}

impl_internable!(Decl, DeclIdent, DeclNameless,);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    pub ident: Decl,
    pub of: Option<Expr>,
    pub val: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    pub ident: Decl,
    pub of: Option<Decl>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    pub lhs: Expr,
    pub key: Decl,
}

impl SelectExpr {
    pub fn new(key: Decl, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { key, lhs })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementExpr {
    pub elem: Element,
    pub content: EcoVec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyExpr {
    pub callee: Expr,
    pub args: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    pub decl: Decl,
    pub params: EcoVec<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    pub pattern: Expr,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowExpr {
    pub selector: Option<Expr>,
    pub edit: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetExpr {
    pub target: Expr,
    pub args: Expr,
    pub cond: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructExpr {
    pub lhs: Expr,
    pub rhs: EcoVec<(Decl, Expr)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportExpr {
    pub decl: Decl,
    pub pattern: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeExpr {
    pub source: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfExpr {
    pub cond: Expr,
    pub then: Expr,
    pub else_: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileExpr {
    pub cond: Expr,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForExpr {
    pub pattern: Expr,
    pub iter: Expr,
    pub body: Expr,
}

/// The kind of unary operation
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum UnaryOp {
    /// The (arithmetic) positive operation
    /// `+t`
    Pos,
    /// The (arithmetic) negate operation
    /// `-t`
    Neg,
    /// The (logical) not operation
    /// `not t`
    Not,
    /// The return operation
    /// `return t`
    Return,
    /// The typst context operation
    /// `context t`
    Context,
    /// The spreading operation
    /// `..t`
    Spread,
    /// The not element of operation
    /// `not in t`
    NotElementOf,
    /// The element of operation
    /// `in t`
    ElementOf,
    /// The type of operation
    /// `type(t)`
    TypeOf,
}

/// A unary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct UnInst<T> {
    /// The operand of the unary operation
    pub lhs: T,
    /// The kind of the unary operation
    pub op: UnaryOp,
}

impl<T: Ord> PartialOrd for UnInst<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for UnInst<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.lhs.cmp(&other.lhs))
    }
}

impl UnInst<Expr> {
    /// Create a unary operation type
    pub fn new(op: UnaryOp, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }
}

impl<T> UnInst<T> {
    /// Get the operands of the unary operation
    pub fn operands(&self) -> [&T; 1] {
        [&self.lhs]
    }
}

/// The kind of binary operation
pub type BinaryOp = ast::BinOp;

/// A binary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct BinInst<T> {
    /// The operands of the binary operation
    pub operands: (T, T),
    /// The kind of the binary operation
    pub op: BinaryOp,
}

impl<T: Ord> PartialOrd for BinInst<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for BinInst<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.operands.cmp(&other.operands))
    }
}

impl BinInst<Expr> {
    /// Create a binary operation type
    pub fn new(op: BinaryOp, lhs: Expr, rhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }
}

impl<T> BinInst<T> {
    /// Get the operands of the binary operation
    pub fn operands(&self) -> [&T; 2] {
        [&self.operands.0, &self.operands.1]
    }
}

impl_internable!(
    Expr,
    ElementExpr,
    ContentSeqExpr,
    RefExpr,
    ContentRefExpr,
    SelectExpr,
    DestructExpr,
    ImportExpr,
    IncludeExpr,
    IfExpr,
    WhileExpr,
    ForExpr,
    FuncExpr,
    LetExpr,
    ShowExpr,
    SetExpr,
    Pattern,
    EcoVec<(Decl, Expr)>,
    EcoVec<Expr>,
    UnInst<Expr>,
    BinInst<Expr>,
    ApplyExpr,
);
