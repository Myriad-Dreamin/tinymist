use core::fmt;
use std::{collections::BTreeMap, ops::Range};

use reflexo_typst::package::PackageSpec;
use serde::{Deserialize, Serialize};
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

use super::{ExprDescriber, ExprPrinter};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A sequence of expressions
    Block(Interned<Vec<Expr>>),
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
    pub(crate) fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_expr(self);
        s
    }

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
        ExprPrinter::new(f).write_expr(self)
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
                crate::log_debug_ct!("evaluating: {name:?} in {scope:?}");
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
                crate::log_debug_ct!("imported: {module:?}");
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

/// Kind of a definition.
#[derive(Debug, Default, Clone, Copy, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefKind {
    /// A definition for some constant.
    #[default]
    Constant,
    /// A definition for some function.
    Function,
    /// A definition for some variable.
    Variable,
    /// A definition for some module.
    Module,
    /// A definition for some struct.
    Struct,
    /// A definition for some reference.
    Reference,
}

impl fmt::Display for DefKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant => write!(f, "constant"),
            Self::Function => write!(f, "function"),
            Self::Variable => write!(f, "variable"),
            Self::Module => write!(f, "module"),
            Self::Struct => write!(f, "struct"),
            Self::Reference => write!(f, "reference"),
        }
    }
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

    pub fn kind(&self) -> DefKind {
        use Decl::*;
        match self {
            ModuleAlias(..) | Module(..) | PathStem(..) | ImportPath(..) | IncludePath(..) => {
                DefKind::Module
            }
            // Type(_) => DocStringKind::Struct,
            Func(..) | Closure(..) => DefKind::Function,
            Label(..) | BibEntry(..) | ContentRef(..) => DefKind::Reference,
            IdentRef(..) | ImportAlias(..) | Import(..) | Var(..) => DefKind::Variable,
            Pattern(..) | Docs(..) | Generated(..) | Constant(..) | StrName(..)
            | ModuleImport(..) | Content(..) | Spread(..) => DefKind::Constant,
        }
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

    pub fn as_def(this: &Interned<Self>, val: Option<Ty>) -> Interned<RefExpr> {
        let def: Expr = this.clone().into();
        Interned::new(RefExpr {
            decl: this.clone(),
            step: Some(def.clone()),
            root: Some(def),
            term: val,
        })
    }
}

impl Ord for Decl {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let base = match (self, other) {
            (Self::Generated(l), Self::Generated(r)) => l.0 .0.cmp(&r.0 .0),
            (Self::Module(l), Self::Module(r)) => l.fid.cmp(&r.fid),
            (Self::Docs(l), Self::Docs(r)) => l.var.cmp(&r.var).then_with(|| l.base.cmp(&r.base)),
            _ => self.span().number().cmp(&other.span().number()),
        };

        base.then_with(|| self.name().cmp(other.name()))
    }
}

impl PartialOrd for Decl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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
    pub name: Interned<str>,
    pub fid: TypstFileId,
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
        ExprPrinter::new(f).write_pattern(self)
    }
}

impl Pattern {
    pub(crate) fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_pattern(self);
        s
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
    pub term: Option<Ty>,
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
    pub decl: Interned<RefExpr>,
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
