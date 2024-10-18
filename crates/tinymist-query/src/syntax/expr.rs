use core::fmt;
use std::{collections::BTreeMap, path::Path};

use rustc_hash::FxHashMap;
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
    Array(Interned<EcoVec<ArgExpr>>),
    /// A dict literal
    Dict(Interned<EcoVec<ArgExpr>>),
    /// An args literal
    Args(Interned<EcoVec<ArgExpr>>),
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
    Import(Interned<ImportExpr>),
    Include(Interned<IncludeExpr>),
    Contextual(Interned<Expr>),
    Conditional(Interned<IfExpr>),
    WhileLoop(Interned<WhileExpr>),
    ForLoop(Interned<ForExpr>),
    Type(Ty),
    Decl(DeclExpr),
    Star,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DefKind {
    Export,
    Func,
    ImportAlias,
    Constant,
    Var,
    BibKey,
    IdentRef,
    Module,
    Import,
    Label,
    Ref,
    StrName,
    PathStem,
    ModuleImport,
    ModuleInclude,
    Spread,
}

pub type DeclExpr = Interned<Decl>;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Decl {
    Export {
        name: Interned<str>,
        fid: TypstFileId,
    },
    Func {
        name: Interned<str>,
        at: Span,
    },
    ImportAlias {
        name: Interned<str>,
        at: Span,
    },
    Var {
        name: Interned<str>,
        at: Span,
    },
    IdentRef {
        name: Interned<str>,
        at: Span,
    },
    Module {
        name: Interned<str>,
        at: Span,
    },
    Import {
        name: Interned<str>,
        at: Span,
    },
    Ref {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    Label {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    StrName {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    PathStem {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    ModuleImport(Span),
    Closure(Span),
    Spread(Span),
}

impl Decl {
    pub fn external(fid: TypstFileId, name: Interned<str>) -> Self {
        Self::Export { fid, name }
    }

    pub fn func(ident: ast::Ident) -> Self {
        Self::Func {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn var(ident: ast::Ident) -> Self {
        Self::Var {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn import_alias(ident: ast::Ident) -> Self {
        Self::ImportAlias {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn ident_ref(ident: ast::Ident) -> Self {
        Self::IdentRef {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn math_ident_ref(ident: ast::MathIdent) -> Self {
        Self::IdentRef {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn module(ident: ast::Ident) -> Self {
        Self::Module {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn import(ident: ast::Ident) -> Self {
        Self::Import {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn label(ident: ast::Label) -> Self {
        Self::Label {
            name: ident.get().into(),
            at: Box::new(ident.to_untyped().clone()),
        }
    }

    pub fn ref_(ident: ast::Ref) -> Self {
        Self::Ref {
            name: ident.target().into(),
            at: Box::new(ident.to_untyped().clone()),
        }
    }

    fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName {
            name: name.into(),
            at: Box::new(s),
        }
    }

    pub fn path_stem(s: SyntaxNode, name: &str) -> Self {
        Self::PathStem {
            name: name.into(),
            at: Box::new(s),
        }
    }

    pub fn name(&self) -> &Interned<str> {
        match self {
            Decl::Export { name, .. }
            | Decl::Func { name, .. }
            | Decl::Var { name, .. }
            | Decl::ImportAlias { name, .. }
            | Decl::IdentRef { name, .. }
            | Decl::Module { name, .. }
            | Decl::Import { name, .. }
            | Decl::Label { name, .. }
            | Decl::Ref { name, .. }
            | Decl::StrName { name, .. }
            | Decl::PathStem { name, .. } => name,
            Decl::ModuleImport(..) | Decl::Closure(..) | Decl::Spread(..) => Interned::empty(),
        }
    }

    pub fn kind(&self) -> DefKind {
        match self {
            Decl::Export { .. } => DefKind::Export,
            Decl::Func { .. } => DefKind::Func,
            Decl::Closure(..) => DefKind::Func,
            Decl::ImportAlias { .. } => DefKind::ImportAlias,
            Decl::Var { .. } => DefKind::Var,
            Decl::IdentRef { .. } => DefKind::IdentRef,
            Decl::Module { .. } => DefKind::Module,
            Decl::Import { .. } => DefKind::Import,
            Decl::Label { .. } => DefKind::Label,
            Decl::Ref { .. } => DefKind::Ref,
            Decl::StrName { .. } => DefKind::StrName,
            Decl::PathStem { .. } => DefKind::PathStem,
            Decl::ModuleImport(..) => DefKind::ModuleImport,
            Decl::Spread(..) => DefKind::Spread,
        }
    }

    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Decl::Export { fid, .. } => Some(*fid),
            that => that.span()?.id(),
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Decl::Export { .. } => None,
            Decl::ModuleImport(at)
            | Decl::Closure(at)
            | Decl::Spread(at)
            | Decl::Func { at, .. }
            | Decl::Var { at, .. }
            | Decl::ImportAlias { at, .. }
            | Decl::IdentRef { at, .. }
            | Decl::Module { at, .. }
            | Decl::Import { at, .. } => Some(*at),
            Decl::Label { at, .. }
            | Decl::Ref { at, .. }
            | Decl::StrName { at, .. }
            | Decl::PathStem { at, .. } => Some(at.span()),
        }
    }

    fn as_def(this: &Interned<Self>, val: Option<Ty>) -> Interned<RefExpr> {
        Interned::new(RefExpr {
            ident: this.clone(),
            of: Some(this.clone().into()),
            val,
        })
    }
}

impl From<Decl> for Expr {
    fn from(decl: Decl) -> Self {
        Expr::Decl(decl.into())
    }
}

impl From<DeclExpr> for Expr {
    fn from(decl: DeclExpr) -> Self {
        Expr::Decl(decl)
    }
}

impl fmt::Debug for Decl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Decl::Ident(ident) => write!(f, "Ident({:?})", ident.name),
            Decl::Export { name, fid } => write!(f, "Export({name:?}, {fid:?})"),
            Decl::Func { name, .. } => write!(f, "Func({name:?})"),
            Decl::Var { name, .. } => write!(f, "Var({name:?})"),
            Decl::ImportAlias { name, .. } => write!(f, "ImportAlias({name:?})"),
            Decl::IdentRef { name, .. } => write!(f, "IdentRef({name:?})"),
            Decl::Module { name, .. } => write!(f, "Module({name:?})"),
            Decl::Import { name, .. } => write!(f, "Import({name:?})"),
            Decl::Label { name, .. } => write!(f, "Label({name:?})"),
            Decl::Ref { name, .. } => write!(f, "Ref({name:?})"),
            Decl::StrName { name, at } => write!(f, "StrName({name:?}, {at:?})"),
            Decl::PathStem { name, at } => write!(f, "PathStem({name:?}, {at:?})"),
            Decl::ModuleImport(..) => write!(f, "ModuleImport(..)"),
            Decl::Closure(..) => write!(f, "Closure(..)"),
            Decl::Spread(..) => write!(f, "Spread(..)"),
        }
    }
}

pub type UnExpr = UnInst<Expr>;
pub type BinExpr = BinInst<Expr>;

#[derive(Debug)]
pub struct ExprInfo {
    pub fid: TypstFileId,
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
            fid: source.id(),
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
        Expr::Decl(Decl::label(ident).into())
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
            Some(name) => Decl::func(name).into(),
            None => Decl::Closure(typed.span()).into(),
        };
        self.resolve_as(Decl::as_def(&decl, None));

        let (params, body) = self.with_scope(|this| {
            this.scope_mut()
                .insert(decl.name().clone(), decl.clone().into());
            let mut inputs = eco_vec![];
            let mut names = eco_vec![];
            let mut spread_left = None;
            let mut spread_right = None;
            for arg in typed.params().children() {
                match arg {
                    ast::Param::Pos(arg) => {
                        inputs.push(this.check_pattern(arg));
                    }
                    ast::Param::Named(arg) => {
                        let key: DeclExpr = Decl::var(arg.name()).into();
                        let val = this.check(arg.expr());
                        names.push((key.clone(), val));

                        this.resolve_as(Decl::as_def(&key, None));
                        this.scope_mut().insert(key.name().clone(), key.into());
                    }
                    ast::Param::Spread(s) => {
                        let decl: DeclExpr = if let Some(ident) = s.sink_ident() {
                            Decl::var(ident).into()
                        } else {
                            Decl::Spread(s.span()).into()
                        };

                        let spreaded = this.check(s.expr());
                        if inputs.is_empty() {
                            spread_left = Some((decl.clone(), spreaded));
                        } else {
                            spread_right = Some((decl.clone(), spreaded));
                        }

                        this.resolve_as(Decl::as_def(&decl, None));
                        this.scope_mut().insert(decl.name().clone(), decl.into());
                    }
                }
            }

            let pattern = Pattern {
                pos: inputs,
                named: names,
                spread_left,
                spread_right,
            };

            (pattern.into(), this.check(typed.body()))
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
                            let key = Decl::var(n.name()).into();
                            let val = self.check_pattern_expr(n.expr());
                            names.push((key, val));
                        }
                        ast::DestructuringItem::Spread(s) => {
                            let decl: DeclExpr = if let Some(ident) = s.sink_ident() {
                                Decl::var(ident).into()
                            } else {
                                Decl::Spread(s.span()).into()
                            };

                            if inputs.is_empty() {
                                spread_left = Some((decl, self.check_pattern_expr(s.expr())));
                            } else {
                                spread_right = Some((decl, self.check_pattern_expr(s.expr())));
                            }
                        }
                    }
                }

                let pattern = Pattern {
                    pos: inputs,
                    named: names,
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
                let decl = Decl::var(ident).into();
                self.resolve_as(Decl::as_def(&decl, None));
                self.scope_mut()
                    .insert(decl.name().clone(), decl.clone().into());
                Expr::Decl(decl)
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
            (Some(ident), _) => Some(Decl::module(ident)),
            (None, Some(Value::Str(i))) if typed.imports().is_none() => {
                let i = Path::new(i.as_str());
                let name = i.file_stem().and_then(|s| s.to_str());
                // Some(self.alloc_path_end(s))
                name.map(|name| Decl::path_stem(source.clone(), name))
            }
            _ => None,
        };
        if let Some(decl) = &decl {
            self.scope_mut()
                .insert(decl.name().clone(), decl.clone().into());
        }
        let decl = decl
            .unwrap_or_else(|| Decl::ModuleImport(typed.span()))
            .into();
        self.resolve_as(Decl::as_def(
            &decl,
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
                    let module = Expr::Decl(decl.clone());

                    for item in items.iter() {
                        let (old, rename) = match item {
                            ast::ImportItem::Simple(ident) => {
                                let old: DeclExpr = Decl::import(ident).into();
                                (old, None)
                            }
                            ast::ImportItem::Renamed(renamed) => {
                                let old: DeclExpr = Decl::import(renamed.original_name()).into();
                                let new: DeclExpr = Decl::import_alias(renamed.new_name()).into();
                                (old, Some(new))
                            }
                        };

                        let old_select = SelectExpr::new(old.clone(), module.clone());
                        self.resolve_as(
                            RefExpr {
                                ident: old.clone(),
                                of: Some(Expr::Select(old_select.clone())),
                                val: None,
                            }
                            .into(),
                        );

                        if let Some(new) = &rename {
                            let rename_ref = RefExpr {
                                ident: new.clone(),
                                of: Some(Expr::Decl(old.clone())),
                                val: None,
                            };
                            self.resolve_as(rename_ref.into());
                        }

                        let new = rename.unwrap_or_else(|| old.clone());
                        let new_name = new.name().clone();
                        let new_expr = Expr::Decl(new);
                        self.scope_mut().insert(new_name, new_expr.clone());
                        imported.push((old, new_expr));
                    }

                    pattern = Expr::Pattern(
                        Pattern {
                            pos: eco_vec![],
                            named: imported,
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
                    items.push(ArgExpr::Pos(self.check(item)));
                }
                ast::ArrayItem::Spread(s) => {
                    items.push(ArgExpr::Spread(self.check(s.expr())));
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
                    let key = Decl::ident_ref(item.name()).into();
                    let val = self.check(item.expr());
                    items.push(ArgExpr::Named(Box::new((key, val))));
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
                    let key = Decl::str_name(key.clone(), analyzed).into();
                    let val = self.check(item.expr());
                    items.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::DictItem::Spread(s) => {
                    items.push(ArgExpr::Spread(self.check(s.expr())));
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
                    args.push(ArgExpr::Pos(self.check(arg)));
                }
                ast::Arg::Named(arg) => {
                    let key = Decl::ident_ref(arg.name()).into();
                    let val = self.check(arg.expr());
                    args.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::Arg::Spread(s) => {
                    args.push(ArgExpr::Spread(self.check(s.expr())));
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
        let rhs = Decl::ident_ref(typed.field()).into();
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
        self.with_scope(|this| {
            let pattern = this.check_pattern(typed.pattern());
            let iter = this.check(typed.iterable());
            let body = this.check(typed.body());
            Expr::ForLoop(
                ForExpr {
                    pattern,
                    iter,
                    body,
                }
                .into(),
            )
        })
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
        let ident = Decl::ref_(r).into();
        let body = r.supplement().map(|s| self.check(ast::Expr::Content(s)));
        let ref_expr = ContentRefExpr {
            ident,
            of: None,
            body,
        };
        Expr::ContentRef(ref_expr.into())
    }

    fn check_ident(&mut self, ident: ast::Ident) -> Expr {
        self.resolve_ident(Decl::ident_ref(ident).into(), InterpretMode::Code)
    }

    fn check_math_ident(&mut self, ident: ast::MathIdent) -> Expr {
        self.resolve_ident(Decl::math_ident_ref(ident).into(), InterpretMode::Code)
    }

    fn resolve_as(&mut self, r: Interned<RefExpr>) {
        let s = r.ident.span().unwrap();
        self.info.resolves.insert(s, r.clone());
    }

    fn resolve_ident(&mut self, decl: DeclExpr, code: InterpretMode) -> Expr {
        let r: Interned<RefExpr> = self.resolve_ident_(decl, code).into();
        let s = r.ident.span().unwrap();
        self.info.resolves.insert(s, r.clone());
        Expr::Ref(r)
    }

    fn resolve_ident_(&mut self, decl: DeclExpr, code: InterpretMode) -> RefExpr {
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
                    if v.is_none() {
                        continue;
                    }

                    let decl = v
                        .and_then(|_| Some(Decl::external(module.file_id()?, name.clone()).into()));

                    (decl, v)
                }
                ExprScope::Func(func) => {
                    let v = func.scope().unwrap().get(&name);
                    if v.is_none() {
                        continue;
                    }
                    (None, v)
                }
                ExprScope::Type(ty) => {
                    let v = ty.scope().get(&name);
                    if v.is_none() {
                        continue;
                    }
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
pub enum ArgExpr {
    Pos(Expr),
    Named(Box<(DeclExpr, Expr)>),
    Spread(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern {
    pub pos: EcoVec<Expr>,
    pub named: EcoVec<(DeclExpr, Expr)>,
    pub spread_left: Option<(DeclExpr, Expr)>,
    pub spread_right: Option<(DeclExpr, Expr)>,
}

impl Pattern {}

impl_internable!(Decl,);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    pub ident: DeclExpr,
    pub of: Option<Expr>,
    pub val: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    pub ident: DeclExpr,
    pub of: Option<DeclExpr>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    pub lhs: Expr,
    pub key: DeclExpr,
}

impl SelectExpr {
    pub fn new(key: DeclExpr, lhs: Expr) -> Interned<Self> {
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
    pub decl: DeclExpr,
    pub params: Interned<Pattern>,
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
pub struct ImportExpr {
    pub decl: DeclExpr,
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
    EcoVec<ArgExpr>,
    EcoVec<Expr>,
    UnInst<Expr>,
    BinInst<Expr>,
    ApplyExpr,
);
