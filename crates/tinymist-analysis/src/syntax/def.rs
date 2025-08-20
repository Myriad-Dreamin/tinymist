use core::fmt;
use std::{
    collections::BTreeMap,
    ops::{Deref, Range},
    sync::Arc,
};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tinymist_derive::DeclEnum;
use tinymist_std::DefId;
use tinymist_world::package::PackageSpec;
use typst::{
    foundations::{Element, Func, Module, Type, Value},
    syntax::{Span, SyntaxNode},
    utils::LazyHash,
};

use crate::{
    adt::interner::impl_internable,
    docs::DocString,
    prelude::*,
    ty::{InsTy, Interned, SelectTy, Ty, TypeVar},
};

use super::{ExprDescriber, ExprPrinter};

/// Represents expression information with lazy evaluation support.
#[derive(Debug, Clone, Hash)]
pub struct ExprInfo(Arc<LazyHash<ExprInfoRepr>>);

impl ExprInfo {
    /// Creates a new expression info wrapper.
    pub fn new(repr: ExprInfoRepr) -> Self {
        Self(Arc::new(LazyHash::new(repr)))
    }
}

impl Deref for ExprInfo {
    type Target = Arc<LazyHash<ExprInfoRepr>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Contains the internal representation of expression information.
#[derive(Debug)]
pub struct ExprInfoRepr {
    /// File identifier.
    pub fid: TypstFileId,
    /// Revision number.
    pub revision: usize,
    /// Source file.
    pub source: Source,
    /// Resolved reference expressions mapped by span.
    pub resolves: FxHashMap<Span, Interned<RefExpr>>,
    /// Module-level documentation string.
    pub module_docstring: Arc<DocString>,
    /// Documentation strings for declarations.
    pub docstrings: FxHashMap<DeclExpr, Arc<DocString>>,
    /// Expressions mapped by span.
    pub exprs: FxHashMap<Span, Expr>,
    /// Imported scopes from other files.
    pub imports: FxHashMap<TypstFileId, Arc<LazyHash<LexicalScope>>>,
    /// Exported scope for this file.
    pub exports: Arc<LazyHash<LexicalScope>>,
    /// Root expression.
    pub root: Expr,
}

impl std::hash::Hash for ExprInfoRepr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.revision.hash(state);
        self.source.hash(state);
        self.exports.hash(state);
        self.root.hash(state);
        let mut resolves = self.resolves.iter().collect::<Vec<_>>();
        resolves.sort_by_key(|(fid, _)| fid.into_raw());
        resolves.hash(state);
        let mut imports = self.imports.iter().collect::<Vec<_>>();
        imports.sort_by_key(|(fid, _)| *fid);
        imports.hash(state);
    }
}

impl ExprInfoRepr {
    /// Gets the definition expression for a declaration.
    pub fn get_def(&self, decl: &Interned<Decl>) -> Option<Expr> {
        if decl.is_def() {
            return Some(Expr::Decl(decl.clone()));
        }
        let resolved = self.resolves.get(&decl.span())?;
        Some(Expr::Ref(resolved.clone()))
    }

    /// Gets references to a given declaration.
    pub fn get_refs(
        &self,
        decl: Interned<Decl>,
    ) -> impl Iterator<Item = (&Span, &Interned<RefExpr>)> {
        let of = Some(Expr::Decl(decl.clone()));
        self.resolves
            .iter()
            .filter(move |(_, r)| match (decl.as_ref(), r.decl.as_ref()) {
                (Decl::Label(..), Decl::Label(..)) => r.decl == decl,
                (Decl::Label(..), Decl::ContentRef(..)) => r.decl.name() == decl.name(),
                (Decl::Label(..), _) => false,
                _ => r.decl == decl || r.root == of,
            })
    }

    /// Checks if a declaration is exported from this module.
    pub fn is_exported(&self, decl: &Interned<Decl>) -> bool {
        let of = Expr::Decl(decl.clone());
        self.exports
            .get(decl.name())
            .is_some_and(|export| match export {
                Expr::Ref(ref_expr) => ref_expr.root == Some(of),
                exprt => *exprt == of,
            })
    }

    #[allow(dead_code)]
    fn show(&self) {
        use std::io::Write;
        let vpath = self
            .fid
            .vpath()
            .resolve(Path::new("target/exprs/"))
            .unwrap();
        let root = vpath.with_extension("root.expr");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(root, format!("{}", self.root)).unwrap();
        let scopes = vpath.with_extension("scopes.expr");
        std::fs::create_dir_all(scopes.parent().unwrap()).unwrap();
        {
            let mut scopes = std::fs::File::create(scopes).unwrap();
            for (span, expr) in self.exprs.iter() {
                writeln!(scopes, "{span:?} -> {expr}").unwrap();
            }
        }
        let imports = vpath.with_extension("imports.expr");
        std::fs::create_dir_all(imports.parent().unwrap()).unwrap();
        std::fs::write(imports, format!("{:#?}", self.imports)).unwrap();
        let exports = vpath.with_extension("exports.expr");
        std::fs::create_dir_all(exports.parent().unwrap()).unwrap();
        std::fs::write(exports, format!("{:#?}", self.exports)).unwrap();
    }
}

/// Represents different types of expressions in the syntax tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A sequence of expressions
    Block(Interned<Vec<Expr>>),
    /// An array literal
    Array(Interned<ArgsExpr>),
    /// A dict literal
    Dict(Interned<ArgsExpr>),
    /// An args literal
    Args(Interned<ArgsExpr>),
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
    /// Gets a textual representation of the expression.
    pub fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_expr(self);
        s
    }

    /// Gets the span of the expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::Decl(decl) => decl.span(),
            Expr::Select(select) => select.span,
            Expr::Apply(apply) => apply.span,
            _ => Span::detached(),
        }
    }

    /// Gets the file identifier of the expression.
    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Expr::Decl(decl) => decl.file_id(),
            _ => self.span().id(),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ExprPrinter::new(f).write_expr(self)
    }
}

/// Type alias for lexical scope mapping names to expressions.
pub type LexicalScope = rpds::RedBlackTreeMapSync<Interned<str>, Expr>;

/// Represents different types of scopes for expressions.
#[derive(Debug, Clone)]
pub enum ExprScope {
    Lexical(LexicalScope),
    Module(Module),
    Func(Func),
    Type(Type),
}

impl ExprScope {
    /// Creates an empty lexical scope.
    pub fn empty() -> Self {
        ExprScope::Lexical(LexicalScope::default())
    }

    /// Checks if the scope is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            ExprScope::Lexical(scope) => scope.is_empty(),
            ExprScope::Module(module) => is_empty_scope(module.scope()),
            ExprScope::Func(func) => func.scope().is_none_or(is_empty_scope),
            ExprScope::Type(ty) => is_empty_scope(ty.scope()),
        }
    }

    /// Gets an expression and type from the scope by name.
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
        (
            of,
            val.cloned()
                .map(|val| Ty::Value(InsTy::new(val.read().to_owned()))),
        )
    }

    /// Merges this scope into the given exports scope.
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
                for (name, _) in module.scope().iter() {
                    let name: Interned<str> = name.into();
                    exports.insert_mut(name.clone(), select_of(v.clone(), name));
                }
            }
            ExprScope::Func(func) => {
                if let Some(scope) = func.scope() {
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Func(func.clone()))));
                    for (name, _) in scope.iter() {
                        let name: Interned<str> = name.into();
                        exports.insert_mut(name.clone(), select_of(v.clone(), name));
                    }
                }
            }
            ExprScope::Type(ty) => {
                let v = Interned::new(Ty::Value(InsTy::new(Value::Type(*ty))));
                for (name, _) in ty.scope().iter() {
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

/// Represents the kind of a definition.
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

/// Type alias for interned declaration expressions.
pub type DeclExpr = Interned<Decl>;

/// Represents different types of declarations in the syntax tree.
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
    /// Creates a function declaration from an identifier.
    pub fn func(ident: ast::Ident) -> Self {
        Self::Func(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a literal variable declaration from a string name.
    pub fn lit(name: &str) -> Self {
        Self::Var(SpannedDecl {
            name: name.into(),
            at: Span::detached(),
        })
    }

    /// Creates a literal variable declaration from an interned string.
    pub fn lit_(name: Interned<str>) -> Self {
        Self::Var(SpannedDecl {
            name,
            at: Span::detached(),
        })
    }

    /// Creates a variable declaration from an identifier.
    pub fn var(ident: ast::Ident) -> Self {
        Self::Var(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an import alias declaration from an identifier.
    pub fn import_alias(ident: ast::Ident) -> Self {
        Self::ImportAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an identifier reference declaration from an identifier.
    pub fn ident_ref(ident: ast::Ident) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an identifier reference declaration from a math identifier.
    pub fn math_ident_ref(ident: ast::MathIdent) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a module declaration with a name and file identifier.
    pub fn module(name: Interned<str>, fid: TypstFileId) -> Self {
        Self::Module(ModuleDecl { name, fid })
    }

    /// Creates a module alias declaration from an identifier.
    pub fn module_alias(ident: ast::Ident) -> Self {
        Self::ModuleAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an import declaration from an identifier.
    pub fn import(ident: ast::Ident) -> Self {
        Self::Import(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a label declaration with a name and span.
    pub fn label(name: &str, at: Span) -> Self {
        Self::Label(SpannedDecl {
            name: name.into(),
            at,
        })
    }

    /// Creates a content reference declaration from a reference AST node.
    pub fn ref_(ident: ast::Ref) -> Self {
        Self::ContentRef(SpannedDecl {
            name: ident.target().into(),
            at: ident.span(),
        })
    }

    /// Creates a string name declaration from a syntax node and name.
    pub fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName(SpannedDecl {
            name: name.into(),
            at: s.span(),
        })
    }

    /// Calculates the path stem from a string path.
    pub fn calc_path_stem(s: &str) -> Interned<str> {
        use std::str::FromStr;
        let name = if s.starts_with('@') {
            let spec = PackageSpec::from_str(s).ok();
            spec.map(|spec| Interned::new_str(spec.name.as_str()))
        } else {
            let stem = Path::new(s).file_stem();
            stem.and_then(|stem| Some(Interned::new_str(stem.to_str()?)))
        };
        name.unwrap_or_default()
    }

    /// Creates a path stem declaration from a syntax node and name.
    pub fn path_stem(s: SyntaxNode, name: Interned<str>) -> Self {
        Self::PathStem(SpannedDecl { name, at: s.span() })
    }

    /// Creates an import path declaration from a span and name.
    pub fn import_path(s: Span, name: Interned<str>) -> Self {
        Self::ImportPath(SpannedDecl { name, at: s })
    }

    /// Creates an include path declaration from a span and name.
    pub fn include_path(s: Span, name: Interned<str>) -> Self {
        Self::IncludePath(SpannedDecl { name, at: s })
    }

    /// Creates a module import declaration from a span.
    pub fn module_import(s: Span) -> Self {
        Self::ModuleImport(SpanDecl(s))
    }

    /// Creates a closure declaration from a span.
    pub fn closure(s: Span) -> Self {
        Self::Closure(SpanDecl(s))
    }

    /// Creates a pattern declaration from a span.
    pub fn pattern(s: Span) -> Self {
        Self::Pattern(SpanDecl(s))
    }

    /// Creates a spread declaration from a span.
    pub fn spread(s: Span) -> Self {
        Self::Spread(SpanDecl(s))
    }

    /// Creates a content declaration from a span.
    pub fn content(s: Span) -> Self {
        Self::Content(SpanDecl(s))
    }

    /// Creates a constant declaration from a span.
    pub fn constant(s: Span) -> Self {
        Self::Constant(SpanDecl(s))
    }

    /// Creates a documentation declaration with a base declaration and type variable.
    pub fn docs(base: Interned<Decl>, var: Interned<TypeVar>) -> Self {
        Self::Docs(DocsDecl { base, var })
    }

    /// Creates a generated declaration from a definition ID.
    pub fn generated(def_id: DefId) -> Self {
        Self::Generated(GeneratedDecl(def_id))
    }

    /// Creates a bibliography entry declaration.
    pub fn bib_entry(
        name: Interned<str>,
        fid: TypstFileId,
        name_range: Range<usize>,
        range: Option<Range<usize>>,
    ) -> Self {
        Self::BibEntry(NameRangeDecl {
            name,
            at: Box::new((fid, name_range, range)),
        })
    }

    /// Checks if this declaration represents a definition.
    pub fn is_def(&self) -> bool {
        matches!(
            self,
            Self::Func(..)
                | Self::BibEntry(..)
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

    /// Gets the kind of this declaration.
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

    /// Gets file location of the declaration.
    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Self::Module(ModuleDecl { fid, .. }) => Some(*fid),
            Self::BibEntry(NameRangeDecl { at, .. }) => Some(at.0),
            that => that.span().id(),
        }
    }

    /// Gets the full range of the declaration.
    pub fn full_range(&self) -> Option<Range<usize>> {
        if let Decl::BibEntry(decl) = self {
            return decl.at.2.clone();
        }

        None
    }

    /// Creates a reference expression from this declaration.
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
            (Self::Generated(l), Self::Generated(r)) => l.0.0.cmp(&r.0.0),
            (Self::Module(l), Self::Module(r)) => l.fid.cmp(&r.fid),
            (Self::Docs(l), Self::Docs(r)) => l.var.cmp(&r.var).then_with(|| l.base.cmp(&r.base)),
            _ => self.span().into_raw().cmp(&other.span().into_raw()),
        };

        base.then_with(|| self.name().cmp(other.name()))
    }
}

trait StrictCmp {
    /// Low-performance comparison but it is free from the concurrency issue.
    /// This is only used for making stable test snapshots.
    fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering;
}

impl Decl {
    /// Performs a strict comparison for stable sorting.
    pub fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        let base = match (self, other) {
            (Self::Generated(l), Self::Generated(r)) => l.0.0.cmp(&r.0.0),
            (Self::Module(l), Self::Module(r)) => l.fid.strict_cmp(&r.fid),
            (Self::Docs(l), Self::Docs(r)) => l
                .var
                .strict_cmp(&r.var)
                .then_with(|| l.base.strict_cmp(&r.base)),
            _ => self.span().strict_cmp(&other.span()),
        };

        base.then_with(|| self.name().cmp(other.name()))
    }
}

impl StrictCmp for TypstFileId {
    fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.package()
            .map(ToString::to_string)
            .cmp(&other.package().map(ToString::to_string))
            .then_with(|| self.vpath().cmp(other.vpath()))
    }
}
impl<T: StrictCmp> StrictCmp for Option<T> {
    fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Some(l), Some(r)) => l.strict_cmp(r),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }
}

impl StrictCmp for Span {
    fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id()
            .strict_cmp(&other.id())
            .then_with(|| self.into_raw().cmp(&other.into_raw()))
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

/// Represents a declaration with a name and span.
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

/// Represents a declaration with a name and range information.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NameRangeDecl {
    /// Name of the declaration.
    pub name: Interned<str>,
    /// File ID, name range, and optional full range.
    pub at: Box<(TypstFileId, Range<usize>, Option<Range<usize>>)>,
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

/// Represents a module declaration with a name and file identifier.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ModuleDecl {
    /// Name of the module.
    pub name: Interned<str>,
    /// File identifier.
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

/// Represents a documentation declaration with a base and type variable.
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

/// Represents a declaration identified only by a span.
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

/// Represents a generated declaration with a definition ID.
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

/// Type alias for unary expressions.
pub type UnExpr = UnInst<Expr>;
/// Type alias for binary expressions.
pub type BinExpr = BinInst<Expr>;

/// Type alias for export maps that map names to expressions.
pub type ExportMap = BTreeMap<Interned<str>, Expr>;

/// Represents different types of argument expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArgExpr {
    Pos(Expr),
    Named(Box<(DeclExpr, Expr)>),
    NamedRt(Box<(Expr, Expr)>),
    Spread(Expr),
}

/// Represents different types of patterns.
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
    /// Gets a textual representation of the pattern.
    pub fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_pattern(self);
        s
    }
}

/// Represents a pattern signature with positional and named parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternSig {
    /// Positional patterns.
    pub pos: EcoVec<Interned<Pattern>>,
    /// Named patterns.
    pub named: EcoVec<(DeclExpr, Interned<Pattern>)>,
    /// Left spread pattern.
    pub spread_left: Option<(DeclExpr, Interned<Pattern>)>,
    /// Right spread pattern.
    pub spread_right: Option<(DeclExpr, Interned<Pattern>)>,
}

impl Pattern {}

impl_internable!(Decl,);

/// Represents a content sequence expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    /// Type of the content sequence.
    pub ty: Ty,
}

/// Represents a reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    /// Declaration being referenced.
    pub decl: DeclExpr,
    /// Optional step in the reference chain.
    pub step: Option<Expr>,
    /// Root expression of the reference.
    pub root: Option<Expr>,
    /// Term type information.
    pub term: Option<Ty>,
}

/// Represents a content reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    /// Identifier being referenced.
    pub ident: DeclExpr,
    /// Optional declaration this refers to.
    pub of: Option<DeclExpr>,
    /// Optional body expression.
    pub body: Option<Expr>,
}

/// Represents a select expression for accessing members.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    /// Left-hand side expression.
    pub lhs: Expr,
    /// Key being selected.
    pub key: DeclExpr,
    /// Span of the select operation.
    pub span: Span,
}

impl SelectExpr {
    /// Creates a new select expression.
    pub fn new(key: DeclExpr, lhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            key,
            lhs,
            span: Span::detached(),
        })
    }
}

/// Represents an arguments expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgsExpr {
    /// List of arguments.
    pub args: Vec<ArgExpr>,
    /// Span of the arguments.
    pub span: Span,
}

impl ArgsExpr {
    /// Creates a new arguments expression.
    pub fn new(span: Span, args: Vec<ArgExpr>) -> Interned<Self> {
        Interned::new(Self { args, span })
    }
}

/// Represents an element expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementExpr {
    /// The element type.
    pub elem: Element,
    /// Content expressions.
    pub content: EcoVec<Expr>,
}

/// Represents a function application expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyExpr {
    /// Function being called.
    pub callee: Expr,
    /// Arguments passed to the function.
    pub args: Expr,
    /// Span of the application.
    pub span: Span,
}

/// Represents a function expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    /// Function declaration.
    pub decl: DeclExpr,
    /// Function parameters.
    pub params: PatternSig,
    /// Function body.
    pub body: Expr,
}

/// Represents a let expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    /// Span of the pattern.
    pub span: Span,
    /// Pattern being bound.
    pub pattern: Interned<Pattern>,
    /// Optional body expression.
    pub body: Option<Expr>,
}

/// Represents a show expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowExpr {
    /// Optional selector expression.
    pub selector: Option<Expr>,
    /// Edit expression.
    pub edit: Expr,
}

/// Represents a set expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetExpr {
    /// Target expression.
    pub target: Expr,
    /// Arguments for the set.
    pub args: Expr,
    /// Optional condition.
    pub cond: Option<Expr>,
}

/// Represents an import expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportExpr {
    /// Declaration of the import.
    pub decl: Interned<RefExpr>,
}

/// Represents an include expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeExpr {
    /// Source expression to include.
    pub source: Expr,
}

/// Represents an if expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfExpr {
    /// Condition expression.
    pub cond: Expr,
    /// Then branch expression.
    pub then: Expr,
    /// Else branch expression.
    pub else_: Expr,
}

/// Represents a while loop expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileExpr {
    /// Condition expression.
    pub cond: Expr,
    /// Body expression.
    pub body: Expr,
}

/// Represents a for loop expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForExpr {
    /// Pattern for iteration variable.
    pub pattern: Interned<Pattern>,
    /// Iterator expression.
    pub iter: Expr,
    /// Body expression.
    pub body: Expr,
}

/// Represents the kinds of unary operations.
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

/// Represents a unary operation with an operand.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct UnInst<T> {
    /// The operand of the unary operation.
    pub lhs: T,
    /// The kind of the unary operation.
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
    /// Creates a unary operation expression.
    pub fn new(op: UnaryOp, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }
}

impl<T> UnInst<T> {
    /// Gets the operands of the unary operation.
    pub fn operands(&self) -> [&T; 1] {
        [&self.lhs]
    }
}

/// Type alias for binary operations from the AST.
pub type BinaryOp = ast::BinOp;

/// Represents a binary operation with two operands.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct BinInst<T> {
    /// The operands of the binary operation.
    pub operands: (T, T),
    /// The kind of the binary operation.
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
    /// Creates a binary operation expression.
    pub fn new(op: BinaryOp, lhs: Expr, rhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }
}

impl<T> BinInst<T> {
    /// Gets the operands of the binary operation.
    pub fn operands(&self) -> [&T; 2] {
        [&self.operands.0, &self.operands.1]
    }
}

fn is_empty_scope(scope: &typst::foundations::Scope) -> bool {
    scope.iter().next().is_none()
}

impl_internable!(
    Expr,
    ArgsExpr,
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
