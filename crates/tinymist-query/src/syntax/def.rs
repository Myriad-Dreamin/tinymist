use core::fmt;
use std::{collections::BTreeMap, ops::Range};

use reflexo_typst::package::PackageSpec;
use tinymist_derive::DeclEnum;
use typst::{
    foundations::{Element, Func, Module, Type, Value},
    syntax::{Span, SyntaxNode},
};

use crate::{
    adt::interner::impl_internable,
    analysis::SharedContext,
    prelude::*,
    ty::{InsTy, Interned, SelectTy, Ty, TypeVar},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A deferred expression
    Defer(DeferExpr),
    /// A sequence of expressions
    Seq(Interned<Vec<Expr>>),
    /// An array literal
    Array(Interned<Vec<ArgExpr>>),
    /// A dict literal
    Dict(Interned<Vec<ArgExpr>>),
    /// An args literal
    Args(Interned<Vec<ArgExpr>>),
    /// A pattern
    Pattern(Interned<Pattern>),
    /// An element literal
    Element(Interned<ElementExpr>),
    /// An unary operation
    Unary(Interned<UnExpr>),
    /// A binary operation
    Binary(Interned<BinExpr>),
    /// A function call
    Apply(Interned<ApplyExpr>),
    /// A function
    Func(Interned<FuncExpr>),
    /// A let
    Let(Interned<LetExpr>),
    /// A show
    Show(Interned<ShowExpr>),
    /// A set
    Set(Interned<SetExpr>),
    /// A reference
    Ref(Interned<RefExpr>),
    /// A content reference
    ContentRef(Interned<ContentRefExpr>),
    /// A select
    Select(Interned<SelectExpr>),
    /// An import
    Import(Interned<ImportExpr>),
    /// An include
    Include(Interned<IncludeExpr>),
    /// A contextual
    Contextual(Interned<Expr>),
    /// A conditional
    Conditional(Interned<IfExpr>),
    /// A while loop
    WhileLoop(Interned<WhileExpr>),
    /// A for loop
    ForLoop(Interned<ForExpr>),
    /// A type
    Type(Ty),
    /// A declaration
    Decl(DeclExpr),
    /// A star import
    Star,
}
impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Decl(d) => d.span(),
            Expr::Select(a) => a.span,
            Expr::Apply(a) => a.span,
            _ => Span::detached(),
        }
    }

    pub(crate) fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Expr::Decl(d) => d.file_id(),
            _ => self.span().id(),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ExprFormatter::new(f).write_expr(self)
    }
}

pub type LexicalScope = rpds::RedBlackTreeMapSync<Interned<str>, Expr>;

#[derive(Debug, Clone)]
pub enum ExprScope {
    Lexical(LexicalScope),
    Module(Module),
    Func(Func),
    Type(Type),
}

impl ExprScope {
    pub fn empty() -> Self {
        ExprScope::Lexical(LexicalScope::default())
    }

    pub fn get(&self, name: &Interned<str>) -> (Option<Expr>, Option<Ty>) {
        let (of, val) = match self {
            ExprScope::Lexical(scope) => {
                log::debug!("evaluating: {name:?} in {scope:?}");
                (scope.get(name).cloned(), None)
            }
            ExprScope::Module(module) => {
                let v = module.scope().get(name);
                // let decl =
                //     v.and_then(|_| Some(Decl::external(module.file_id()?,
                // name.clone()).into()));
                (None, v)
            }
            ExprScope::Func(func) => (None, func.scope().unwrap().get(name)),
            ExprScope::Type(ty) => (None, ty.scope().get(name)),
        };

        // ref_expr.of = of.clone();
        // ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
        // return ref_expr;
        (of, val.cloned().map(|val| Ty::Value(InsTy::new(val))))
    }

    pub fn merge_into(&self, exports: &mut LexicalScope) {
        match self {
            ExprScope::Lexical(scope) => {
                for (name, expr) in scope.iter() {
                    exports.insert_mut(name.clone(), expr.clone());
                }
            }
            ExprScope::Module(module) => {
                log::debug!("imported: {module:?}");
                let v = Interned::new(Ty::Value(InsTy::new(Value::Module(module.clone()))));
                for (name, _, _) in module.scope().iter() {
                    let name: Interned<str> = name.into();
                    exports.insert_mut(name.clone(), select_of(v.clone(), name));
                }
            }
            ExprScope::Func(func) => {
                if let Some(scope) = func.scope() {
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Func(func.clone()))));
                    for (name, _, _) in scope.iter() {
                        let name: Interned<str> = name.into();
                        exports.insert_mut(name.clone(), select_of(v.clone(), name));
                    }
                }
            }
            ExprScope::Type(ty) => {
                let v = Interned::new(Ty::Value(InsTy::new(Value::Type(*ty))));
                for (name, _, _) in ty.scope().iter() {
                    let name: Interned<str> = name.into();
                    exports.insert_mut(name.clone(), select_of(v.clone(), name));
                }
            }
        }
    }
}

fn select_of(source: Interned<Ty>, name: Interned<str>) -> Expr {
    Expr::Type(Ty::Select(SelectTy::new(source, name)))
}

pub type DeclExpr = Interned<Decl>;

#[derive(Clone, PartialEq, Eq, Hash, DeclEnum)]
pub enum Decl {
    Func(SpannedDecl),
    ImportAlias(SpannedDecl),
    Var(SpannedDecl),
    IdentRef(SpannedDecl),
    Module(ModuleDecl),
    ModuleAlias(SpannedDecl),
    PathStem(SpannedDecl),
    ImportPath(SpannedDecl),
    IncludePath(SpannedDecl),
    Import(SpannedDecl),
    ContentRef(SpannedDecl),
    Label(SpannedDecl),
    StrName(SpannedDecl),
    ModuleImport(SpanDecl),
    Closure(SpanDecl),
    Pattern(SpanDecl),
    Spread(SpanDecl),
    Content(SpanDecl),
    Constant(SpanDecl),
    BibEntry(NameRangeDecl),
    Docs(DocsDecl),
    Generated(GeneratedDecl),
}

impl Decl {
    pub fn func(ident: ast::Ident) -> Self {
        Self::Func(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn lit(name: &str) -> Self {
        Self::Var(SpannedDecl {
            name: name.into(),
            at: Span::detached(),
        })
    }

    pub fn lit_(name: Interned<str>) -> Self {
        Self::Var(SpannedDecl {
            name,
            at: Span::detached(),
        })
    }

    pub fn var(ident: ast::Ident) -> Self {
        Self::Var(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn import_alias(ident: ast::Ident) -> Self {
        Self::ImportAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn ident_ref(ident: ast::Ident) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn math_ident_ref(ident: ast::MathIdent) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn module(name: Interned<str>, fid: TypstFileId) -> Self {
        Self::Module(ModuleDecl { name, fid })
    }

    pub fn module_alias(ident: ast::Ident) -> Self {
        Self::ModuleAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn import(ident: ast::Ident) -> Self {
        Self::Import(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    pub fn label(name: &str, at: Span) -> Self {
        Self::Label(SpannedDecl {
            name: name.into(),
            at,
        })
    }

    pub fn ref_(ident: ast::Ref) -> Self {
        Self::ContentRef(SpannedDecl {
            name: ident.target().into(),
            at: ident.span(),
        })
    }

    pub fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName(SpannedDecl {
            name: name.into(),
            at: s.span(),
        })
    }

    pub fn calc_path_stem(s: &str) -> Interned<str> {
        use std::str::FromStr;
        let name = if s.starts_with('@') {
            let spec = PackageSpec::from_str(s).ok();
            spec.map(|p| Interned::new_str(p.name.as_str()))
        } else {
            let stem = Path::new(s).file_stem();
            stem.and_then(|s| Some(Interned::new_str(s.to_str()?)))
        };
        name.unwrap_or_default()
    }

    pub fn path_stem(s: SyntaxNode, name: Interned<str>) -> Self {
        Self::PathStem(SpannedDecl { name, at: s.span() })
    }

    pub fn import_path(s: Span, name: Interned<str>) -> Self {
        Self::ImportPath(SpannedDecl { name, at: s })
    }

    pub fn include_path(s: Span, name: Interned<str>) -> Self {
        Self::IncludePath(SpannedDecl { name, at: s })
    }

    pub fn module_import(s: Span) -> Self {
        Self::ModuleImport(SpanDecl(s))
    }

    pub fn closure(s: Span) -> Self {
        Self::Closure(SpanDecl(s))
    }

    pub fn pattern(s: Span) -> Self {
        Self::Pattern(SpanDecl(s))
    }

    pub fn spread(s: Span) -> Self {
        Self::Spread(SpanDecl(s))
    }

    pub fn content(s: Span) -> Self {
        Self::Content(SpanDecl(s))
    }

    pub fn constant(s: Span) -> Self {
        Self::Constant(SpanDecl(s))
    }

    pub fn docs(base: Interned<Decl>, var: Interned<TypeVar>) -> Self {
        Self::Docs(DocsDecl { base, var })
    }

    pub fn generated(def_id: DefId) -> Self {
        Self::Generated(GeneratedDecl(def_id))
    }

    pub fn bib_entry(name: Interned<str>, fid: TypstFileId, range: Range<usize>) -> Self {
        Self::BibEntry(NameRangeDecl {
            name,
            at: Box::new((fid, range)),
        })
    }

    pub(crate) fn is_def(&self) -> bool {
        matches!(
            self,
            Self::Func(..)
                | Self::Closure(..)
                | Self::Var(..)
                | Self::Label(..)
                | Self::StrName(..)
                | Self::Module(..)
                | Self::ModuleImport(..)
                | Self::PathStem(..)
                | Self::ImportPath(..)
                | Self::IncludePath(..)
                | Self::Spread(..)
                | Self::Generated(..)
        )
    }

    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Self::Module(ModuleDecl { fid, .. }) => Some(*fid),
            that => that.span().id(),
        }
    }

    // todo: name range
    /// The range of the name of the definition.
    pub fn name_range(&self, ctx: &SharedContext) -> Option<Range<usize>> {
        if !self.is_def() {
            return None;
        }

        let fid = self.file_id()?;
        let src = ctx.source_by_id(fid).ok()?;
        src.range(self.span())
    }

    pub fn weak_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name()
            .cmp(other.name())
            .then_with(|| match (self, other) {
                (Self::Generated(l), Self::Generated(r)) => l.0 .0.cmp(&r.0 .0),
                (Self::Docs(l), Self::Docs(r)) => {
                    l.var.cmp(&r.var).then_with(|| l.base.weak_cmp(&r.base))
                }
                _ => self.span().number().cmp(&other.span().number()),
            })
    }

    pub fn as_def(this: &Interned<Self>, val: Option<Ty>) -> Interned<RefExpr> {
        let def: Expr = this.clone().into();
        Interned::new(RefExpr {
            decl: this.clone(),
            step: Some(def.clone()),
            root: Some(def),
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

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpannedDecl {
    name: Interned<str>,
    at: Span,
}

impl SpannedDecl {
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    fn span(&self) -> Span {
        self.at
    }
}

impl fmt::Debug for SpannedDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.as_ref())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NameRangeDecl {
    name: Interned<str>,
    at: Box<(TypstFileId, Range<usize>)>,
}

impl NameRangeDecl {
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    fn span(&self) -> Span {
        Span::detached()
    }
}

impl fmt::Debug for NameRangeDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.as_ref())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ModuleDecl {
    name: Interned<str>,
    fid: TypstFileId,
}

impl ModuleDecl {
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    fn span(&self) -> Span {
        Span::detached()
    }
}

impl fmt::Debug for ModuleDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.as_ref())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocsDecl {
    base: Interned<Decl>,
    var: Interned<TypeVar>,
}

impl DocsDecl {
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    fn span(&self) -> Span {
        Span::detached()
    }
}

impl fmt::Debug for DocsDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}, {:?}", self.base, self.var)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpanDecl(Span);

impl SpanDecl {
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    fn span(&self) -> Span {
        self.0
    }
}

impl fmt::Debug for SpanDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "..")
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GeneratedDecl(DefId);

impl GeneratedDecl {
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    fn span(&self) -> Span {
        Span::detached()
    }
}

impl fmt::Debug for GeneratedDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub type UnExpr = UnInst<Expr>;
pub type BinExpr = BinInst<Expr>;

pub type ExportMap = BTreeMap<Interned<str>, Expr>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArgExpr {
    Pos(Expr),
    Named(Box<(DeclExpr, Expr)>),
    NamedRt(Box<(Expr, Expr)>),
    Spread(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Expr(Expr),
    Simple(Interned<Decl>),
    Sig(Box<PatternSig>),
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        ExprFormatter::new(f).write_pattern(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternSig {
    pub pos: EcoVec<Interned<Pattern>>,
    pub named: EcoVec<(DeclExpr, Interned<Pattern>)>,
    pub spread_left: Option<(DeclExpr, Interned<Pattern>)>,
    pub spread_right: Option<(DeclExpr, Interned<Pattern>)>,
}

impl Pattern {}

impl_internable!(Decl,);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    pub decl: DeclExpr,
    pub step: Option<Expr>,
    pub root: Option<Expr>,
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
    pub span: Span,
}

impl SelectExpr {
    pub fn new(key: DeclExpr, lhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            key,
            lhs,
            span: Span::detached(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeferExpr {
    pub span: Span,
}

impl From<DeferExpr> for Expr {
    fn from(defer: DeferExpr) -> Self {
        Expr::Defer(defer)
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
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    pub decl: DeclExpr,
    pub params: PatternSig,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    /// Span of the pattern
    pub span: Span,
    pub pattern: Interned<Pattern>,
    pub body: Option<Expr>,
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
    pub pattern: Interned<Pattern>,
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
    Vec<ArgExpr>,
    Vec<Expr>,
    UnInst<Expr>,
    BinInst<Expr>,
    ApplyExpr,
);

struct ExprFormatter<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    indent: usize,
}

impl<'a, 'b> ExprFormatter<'a, 'b> {
    fn new(f: &'a mut fmt::Formatter<'b>) -> Self {
        Self { f, indent: 0 }
    }

    fn write_decl(&mut self, d: &Decl) -> fmt::Result {
        write!(self.f, "{d:?}")
    }

    fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Defer(..) => write!(self.f, "defer(..)"),
            Expr::Seq(s) => self.write_seq(s),
            Expr::Array(a) => self.write_array(a),
            Expr::Dict(d) => self.write_dict(d),
            Expr::Args(a) => self.write_args(a),
            Expr::Pattern(p) => self.write_pattern(p),
            Expr::Element(e) => self.write_element(e),
            Expr::Unary(u) => self.write_unary(u),
            Expr::Binary(b) => self.write_binary(b),
            Expr::Apply(a) => self.write_apply(a),
            Expr::Func(func) => self.write_func(func),
            Expr::Let(l) => self.write_let(l),
            Expr::Show(s) => self.write_show(s),
            Expr::Set(s) => self.write_set(s),
            Expr::Ref(r) => self.write_ref(r),
            Expr::ContentRef(r) => self.write_content_ref(r),
            Expr::Select(s) => self.write_select(s),
            Expr::Import(i) => self.write_import(i),
            Expr::Include(i) => self.write_include(i),
            Expr::Contextual(c) => self.write_contextual(c),
            Expr::Conditional(c) => self.write_conditional(c),
            Expr::WhileLoop(w) => self.write_while_loop(w),
            Expr::ForLoop(f) => self.write_for_loop(f),
            Expr::Type(t) => self.write_type(t),
            Expr::Decl(d) => self.write_decl(d),
            Expr::Star => self.write_star(),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_seq(&mut self, s: &Interned<Vec<Expr>>) -> fmt::Result {
        writeln!(self.f, "[")?;
        self.indent += 1;
        for expr in s.iter() {
            self.write_indent()?;
            self.write_expr(expr)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, "]")
    }

    fn write_array(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, d: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in d.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, a: &ArgExpr) -> fmt::Result {
        match a {
            ArgExpr::Pos(e) => self.write_expr(e),
            ArgExpr::Named(n) => {
                let n = n.as_ref();
                write!(self.f, "{n:?}: ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::NamedRt(n) => {
                let n = n.as_ref();
                self.write_expr(&n.0)?;
                write!(self.f, ": ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::Spread(e) => {
                write!(self.f, "..")?;
                self.write_expr(e)
            }
        }
    }

    fn write_pattern(&mut self, p: &Pattern) -> fmt::Result {
        match p {
            Pattern::Expr(e) => self.write_expr(e),
            Pattern::Simple(s) => self.write_decl(s),
            Pattern::Sig(p) => self.write_pattern_sig(p),
        }
    }

    fn write_pattern_sig(&mut self, p: &PatternSig) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &p.pos {
            self.write_indent()?;
            self.write_pattern(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, pat) in &p.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_pattern(pat)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_left {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_right {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, e: &Interned<ElementExpr>) -> fmt::Result {
        self.f.write_str("elem(\n")?;
        self.indent += 1;
        for v in &e.content {
            self.write_indent()?;
            self.write_expr(v)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_unary(&mut self, u: &Interned<UnExpr>) -> fmt::Result {
        write!(self.f, "un({:?})(", u.op)?;
        self.write_expr(&u.lhs)?;
        self.f.write_str(")")
    }

    fn write_binary(&mut self, b: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = b.operands();
        write!(self.f, "bin({:?})(", b.op)?;
        self.write_expr(lhs)?;
        self.f.write_str(", ")?;
        self.write_expr(rhs)?;
        self.f.write_str(")")
    }

    fn write_apply(&mut self, a: &Interned<ApplyExpr>) -> fmt::Result {
        write!(self.f, "apply(")?;
        self.write_expr(&a.callee)?;
        self.f.write_str(", ")?;
        self.write_expr(&a.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        write!(self.f, "func[{:?}](", func.decl)?;
        self.write_pattern_sig(&func.params)?;
        write!(self.f, " = ")?;
        self.write_expr(&func.body)?;
        write!(self.f, ")")
    }

    fn write_let(&mut self, l: &Interned<LetExpr>) -> fmt::Result {
        write!(self.f, "let(")?;
        self.write_pattern(&l.pattern)?;
        if let Some(body) = &l.body {
            write!(self.f, " = ")?;
            self.write_expr(body)?;
        }
        write!(self.f, ")")
    }

    fn write_show(&mut self, s: &Interned<ShowExpr>) -> fmt::Result {
        write!(self.f, "show(")?;
        if let Some(selector) = &s.selector {
            self.write_expr(selector)?;
            self.f.write_str(", ")?;
        }
        self.write_expr(&s.edit)?;
        write!(self.f, ")")
    }

    fn write_set(&mut self, s: &Interned<SetExpr>) -> fmt::Result {
        write!(self.f, "set(")?;
        self.write_expr(&s.target)?;
        self.f.write_str(", ")?;
        self.write_expr(&s.args)?;
        if let Some(cond) = &s.cond {
            self.f.write_str(", ")?;
            self.write_expr(cond)?;
        }
        write!(self.f, ")")
    }

    fn write_ref(&mut self, r: &Interned<RefExpr>) -> fmt::Result {
        write!(self.f, "ref({:?}", r.decl)?;
        if let Some(step) = &r.step {
            self.f.write_str(", step = ")?;
            self.write_expr(step)?;
        }
        if let Some(of) = &r.root {
            self.f.write_str(", root = ")?;
            self.write_expr(of)?;
        }
        if let Some(val) = &r.val {
            write!(self.f, ", val = {val:?}")?;
        }
        self.f.write_str(")")
    }

    fn write_content_ref(&mut self, r: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "content_ref({:?}", r.ident)?;
        if let Some(of) = &r.of {
            self.f.write_str(", ")?;
            self.write_decl(of)?;
        }
        if let Some(val) = &r.body {
            self.write_expr(val)?;
        }
        self.f.write_str(")")
    }

    fn write_select(&mut self, s: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "(")?;
        self.write_expr(&s.lhs)?;
        self.f.write_str(").")?;
        self.write_decl(&s.key)
    }

    fn write_import(&mut self, i: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&i.decl)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, i: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&i.source)?;
        self.f.write_str(")")
    }

    fn write_contextual(&mut self, c: &Interned<Expr>) -> fmt::Result {
        self.f.write_str("contextual(")?;
        self.write_expr(c)?;
        self.f.write_str(")")
    }

    fn write_conditional(&mut self, c: &Interned<IfExpr>) -> fmt::Result {
        self.f.write_str("if(")?;
        self.write_expr(&c.cond)?;
        self.f.write_str(", then = ")?;
        self.write_expr(&c.then)?;
        self.f.write_str(", else = ")?;
        self.write_expr(&c.else_)?;
        self.f.write_str(")")
    }

    fn write_while_loop(&mut self, w: &Interned<WhileExpr>) -> fmt::Result {
        self.f.write_str("while(")?;
        self.write_expr(&w.cond)?;
        self.f.write_str(", ")?;
        self.write_expr(&w.body)?;
        self.f.write_str(")")
    }

    fn write_for_loop(&mut self, f: &Interned<ForExpr>) -> fmt::Result {
        self.f.write_str("for(")?;
        self.write_pattern(&f.pattern)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.iter)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.body)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, t: &Ty) -> fmt::Result {
        let formatted = t.describe();
        let formatted = formatted.as_deref().unwrap_or("any");
        self.f.write_str(formatted)
    }

    fn write_star(&mut self) -> fmt::Result {
        self.f.write_str("*")
    }
}
