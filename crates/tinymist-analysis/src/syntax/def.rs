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

/// Information about expressions in a source file.
///
/// This structure wraps expression analysis data and provides access to
/// expression resolution, documentation, and scoping information.
#[derive(Debug, Clone, Hash)]
pub struct ExprInfo(Arc<LazyHash<ExprInfoRepr>>);

impl ExprInfo {
    /// Creates a new ExprInfo instance from expression information representation.
    ///
    /// Wraps the provided representation in an Arc and LazyHash for efficient sharing and hashing.
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

/// Representation of expression information for a specific file.
///
/// Contains all the analyzed information about expressions in a source file,
/// including resolution maps, documentation strings, imports, and exports.
#[derive(Debug)]
pub struct ExprInfoRepr {
    /// The file ID this expression information belongs to
    pub fid: TypstFileId,
    /// Revision number for tracking changes to the file
    pub revision: usize,
    /// The source code content
    pub source: Source,
    /// Map from spans to resolved reference expressions
    pub resolves: FxHashMap<Span, Interned<RefExpr>>,
    /// Documentation string for the module
    pub module_docstring: Arc<DocString>,
    /// Map from declarations to their documentation strings
    pub docstrings: FxHashMap<DeclExpr, Arc<DocString>>,
    /// Map from spans to expressions for scope analysis
    pub exprs: FxHashMap<Span, Expr>,
    /// Map from file IDs to imported lexical scopes
    pub imports: FxHashMap<TypstFileId, Arc<LazyHash<LexicalScope>>>,
    /// The lexical scope of exported symbols from this file
    pub exports: Arc<LazyHash<LexicalScope>>,
    /// The root expression of the file
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
    /// Gets the definition expression for a given declaration.
    ///
    /// Returns the declaration itself if it's a definition, otherwise
    /// looks up the resolved reference expression.
    pub fn get_def(&self, decl: &Interned<Decl>) -> Option<Expr> {
        if decl.is_def() {
            return Some(Expr::Decl(decl.clone()));
        }
        let resolved = self.resolves.get(&decl.span())?;
        Some(Expr::Ref(resolved.clone()))
    }

    /// Gets all references to a given declaration.
    ///
    /// Returns an iterator over spans and reference expressions that
    /// reference the provided declaration, with special handling for labels.
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
    ///
    /// Returns true if the declaration appears in the module's export scope.
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

/// Represents different kinds of expressions in the language.
///
/// This enum covers all possible expression types that can appear in Typst
/// source code, from basic literals to complex control flow constructs.
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
    /// Returns a string representation of the expression.
    ///
    /// Uses the ExprDescriber to create a human-readable representation
    /// of the expression for debugging and display purposes.
    pub fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_expr(self);
        s
    }

    /// Returns the span location of the expression.
    ///
    /// For most expressions returns a detached span, but declarations,
    /// selections, and applications have specific span information.
    pub fn span(&self) -> Span {
        match self {
            Expr::Decl(decl) => decl.span(),
            Expr::Select(select) => select.span,
            Expr::Apply(apply) => apply.span,
            _ => Span::detached(),
        }
    }

    /// Returns the file ID associated with this expression, if any.
    ///
    /// Attempts to extract file ID from declarations first, then falls back
    /// to the span's file ID.
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

/// Type alias for lexical scopes.
///
/// Represents a lexical scope as a persistent map from names to expressions.
pub type LexicalScope = rpds::RedBlackTreeMapSync<Interned<str>, Expr>;

/// Different types of scopes for expression evaluation.
///
/// Represents the various kinds of scopes that can contain variable bindings,
/// including lexical scopes, modules, functions, and types.
#[derive(Debug, Clone)]
pub enum ExprScope {
    Lexical(LexicalScope),
    Module(Module),
    Func(Func),
    Type(Type),
}

impl ExprScope {
    /// Creates an empty lexical scope.
    ///
    /// Returns a new ExprScope containing an empty lexical scope map.
    pub fn empty() -> Self {
        ExprScope::Lexical(LexicalScope::default())
    }

    /// Checks if the scope contains no bindings.
    ///
    /// Returns true if the scope has no variables, functions, or other bindings.
    pub fn is_empty(&self) -> bool {
        match self {
            ExprScope::Lexical(scope) => scope.is_empty(),
            ExprScope::Module(module) => is_empty_scope(module.scope()),
            ExprScope::Func(func) => func.scope().is_none_or(is_empty_scope),
            ExprScope::Type(ty) => is_empty_scope(ty.scope()),
        }
    }

    /// Looks up a name in the scope and returns both expression and type information.
    ///
    /// Returns a tuple of (expression, type) where either may be None depending
    /// on the scope type and whether the name is found.
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

    /// Merges all bindings from this scope into the provided export map.
    ///
    /// Copies all name-expression bindings from this scope into the exports,
    /// converting values from modules, functions, and types as needed.
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

/// Kind of a definition.
///
/// Classifies different types of definitions that can appear in source code
/// for language server features like symbols and completion.
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

/// Type alias for declaration expressions.
///
/// Represents an interned declaration that can be referenced throughout the analysis.
pub type DeclExpr = Interned<Decl>;

/// Represents different kinds of declarations in the language.
///
/// This enum covers all possible declaration types, from function and variable
/// declarations to imports, labels, and generated definitions.
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
    ///
    /// Extracts the name and span from the AST identifier to create
    /// a function declaration.
    pub fn func(ident: ast::Ident) -> Self {
        Self::Func(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a variable declaration from a string literal.
    ///
    /// Creates a variable declaration with the given name and a detached span.
    /// Used for synthetic or generated variable declarations.
    pub fn lit(name: &str) -> Self {
        Self::Var(SpannedDecl {
            name: name.into(),
            at: Span::detached(),
        })
    }

    /// Creates a variable declaration from an interned string.
    ///
    /// Similar to `lit` but takes an already interned string name.
    /// Used for synthetic or generated variable declarations.
    pub fn lit_(name: Interned<str>) -> Self {
        Self::Var(SpannedDecl {
            name,
            at: Span::detached(),
        })
    }

    /// Creates a variable declaration from an identifier.
    ///
    /// Extracts the name and span from the AST identifier to create
    /// a variable declaration.
    pub fn var(ident: ast::Ident) -> Self {
        Self::Var(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an import alias declaration from an identifier.
    ///
    /// Used when an import statement creates an alias for the imported item.
    pub fn import_alias(ident: ast::Ident) -> Self {
        Self::ImportAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an identifier reference declaration from an identifier.
    ///
    /// Used for references to identifiers in expressions, not definitions.
    pub fn ident_ref(ident: ast::Ident) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an identifier reference declaration from a math identifier.
    ///
    /// Similar to `ident_ref` but for mathematical identifiers in math mode.
    pub fn math_ident_ref(ident: ast::MathIdent) -> Self {
        Self::IdentRef(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a module declaration with a name and file ID.
    ///
    /// Associates a module name with its corresponding file ID for imports and references.
    pub fn module(name: Interned<str>, fid: TypstFileId) -> Self {
        Self::Module(ModuleDecl { name, fid })
    }

    /// Creates a module alias declaration from an identifier.
    ///
    /// Used when importing a module with an alias (e.g., `import "file.typ" as alias`).
    pub fn module_alias(ident: ast::Ident) -> Self {
        Self::ModuleAlias(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates an import declaration from an identifier.
    ///
    /// Represents a name being imported from another module.
    pub fn import(ident: ast::Ident) -> Self {
        Self::Import(SpannedDecl {
            name: ident.get().into(),
            at: ident.span(),
        })
    }

    /// Creates a label declaration with a name and span.
    ///
    /// Used for labels that can be referenced by content references.
    pub fn label(name: &str, at: Span) -> Self {
        Self::Label(SpannedDecl {
            name: name.into(),
            at,
        })
    }

    /// Creates a content reference declaration from a reference AST node.
    ///
    /// Extracts the target name and span from the reference to create
    /// a content reference declaration.
    pub fn ref_(ident: ast::Ref) -> Self {
        Self::ContentRef(SpannedDecl {
            name: ident.target().into(),
            at: ident.span(),
        })
    }

    /// Creates a string name declaration from a syntax node and name.
    ///
    /// Used for declarations identified by string names in specific contexts.
    pub fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName(SpannedDecl {
            name: name.into(),
            at: s.span(),
        })
    }

    /// Calculates the path stem from a string path or package specification.
    ///
    /// For package specs (starting with '@'), extracts the package name.
    /// For file paths, extracts the file stem. Returns empty string if extraction fails.
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
    ///
    /// Used for representing the stem of an import or include path.
    pub fn path_stem(s: SyntaxNode, name: Interned<str>) -> Self {
        Self::PathStem(SpannedDecl { name, at: s.span() })
    }

    /// Creates an import path declaration with a span and name.
    ///
    /// Represents a path being imported in an import statement.
    pub fn import_path(s: Span, name: Interned<str>) -> Self {
        Self::ImportPath(SpannedDecl { name, at: s })
    }

    /// Creates an include path declaration with a span and name.
    ///
    /// Represents a path being included in an include statement.
    pub fn include_path(s: Span, name: Interned<str>) -> Self {
        Self::IncludePath(SpannedDecl { name, at: s })
    }

    /// Creates a module import declaration with just a span.
    ///
    /// Used for anonymous module imports that don't have specific names.
    pub fn module_import(s: Span) -> Self {
        Self::ModuleImport(SpanDecl(s))
    }

    /// Creates a closure declaration with just a span.
    ///
    /// Represents an anonymous function or closure definition.
    pub fn closure(s: Span) -> Self {
        Self::Closure(SpanDecl(s))
    }

    /// Creates a pattern declaration with just a span.
    ///
    /// Used for pattern matching constructs in let bindings and function parameters.
    pub fn pattern(s: Span) -> Self {
        Self::Pattern(SpanDecl(s))
    }

    /// Creates a spread declaration with just a span.
    ///
    /// Represents a spread operator in argument lists or destructuring.
    pub fn spread(s: Span) -> Self {
        Self::Spread(SpanDecl(s))
    }

    /// Creates a content declaration with just a span.
    ///
    /// Used for content elements that don't have specific identifiers.
    pub fn content(s: Span) -> Self {
        Self::Content(SpanDecl(s))
    }

    /// Creates a constant declaration with just a span.
    ///
    /// Represents constant values or expressions.
    pub fn constant(s: Span) -> Self {
        Self::Constant(SpanDecl(s))
    }

    /// Creates a documentation declaration linking a base declaration with a type variable.
    ///
    /// Used for associating documentation with specific type instantiations.
    pub fn docs(base: Interned<Decl>, var: Interned<TypeVar>) -> Self {
        Self::Docs(DocsDecl { base, var })
    }

    /// Creates a generated declaration with a definition ID.
    ///
    /// Used for declarations that are created programmatically rather than from source.
    pub fn generated(def_id: DefId) -> Self {
        Self::Generated(GeneratedDecl(def_id))
    }

    /// Creates a bibliography entry declaration.
    ///
    /// Used for citations and bibliography entries with precise range information.
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

    /// Checks if this declaration represents a definition rather than a reference.
    ///
    /// Returns true for declarations that define new symbols (functions, variables, labels, etc.)
    /// and false for references to existing symbols.
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

    /// Returns the kind of definition this declaration represents.
    ///
    /// Classifies the declaration into categories like function, variable, module, etc.
    /// for use in language server features.
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
    ///
    /// Returns the file ID where this declaration is located, if available.
    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Self::Module(ModuleDecl { fid, .. }) => Some(*fid),
            Self::BibEntry(NameRangeDecl { at, .. }) => Some(at.0),
            that => that.span().id(),
        }
    }

    /// Gets full range of the declaration.
    ///
    /// Returns the complete range in the source file for declarations that track it,
    /// currently only bibliography entries.
    pub fn full_range(&self) -> Option<Range<usize>> {
        if let Decl::BibEntry(decl) = self {
            return decl.at.2.clone();
        }

        None
    }

    /// Creates a reference expression that points to this declaration.
    ///
    /// Constructs a RefExpr that represents a reference to this declaration,
    /// optionally with type information.
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
    /// Low-performance comparison that is free from concurrency issues.
    ///
    /// This comparison method is only used for making stable test snapshots
    /// and avoids potential race conditions in concurrent scenarios.
    pub fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        let base = match (self, other) {
            (Self::Generated(l), Self::Generated(r)) => l.0 .0.cmp(&r.0 .0),
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

/// A declaration with an associated name and span location.
///
/// Used for most declaration types that have a simple name-span relationship.
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

/// A declaration with a name and range information.
///
/// Used for declarations that need precise range information, such as bibliography entries.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NameRangeDecl {
    /// The name of the declaration
    pub name: Interned<str>,
    /// Boxed tuple containing (file_id, name_range, full_range)
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

/// A module declaration with name and file ID.
///
/// Represents a module that can be imported or referenced.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ModuleDecl {
    /// The name of the module
    pub name: Interned<str>,
    /// The file ID where the module is defined
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

/// A documentation declaration linking a base declaration with type variables.
///
/// Used for associating documentation with specific type instantiations.
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

/// A span-only declaration for anonymous constructs.
///
/// Used for declarations that don't have names but need location tracking.
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

/// A generated declaration with a unique definition ID.
///
/// Used for declarations that are created programmatically rather than from source.
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

/// Type alias for export maps.
///
/// Maps exported names to their corresponding expressions.
pub type ExportMap = BTreeMap<Interned<str>, Expr>;

/// Represents different kinds of function arguments.
///
/// Covers positional arguments, named arguments, and spread arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArgExpr {
    Pos(Expr),
    Named(Box<(DeclExpr, Expr)>),
    NamedRt(Box<(Expr, Expr)>),
    Spread(Expr),
}

/// Represents different kinds of patterns for destructuring.
///
/// Used in let bindings, function parameters, and other pattern-matching contexts.
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
    /// Returns a string representation of the pattern.
    ///
    /// Uses the ExprDescriber to create a human-readable representation
    /// of the pattern for debugging and display purposes.
    pub fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_pattern(self);
        s
    }
}

/// Signature pattern for function parameters.
///
/// Describes the structure of function parameters including positional,
/// named, and spread parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PatternSig {
    /// Positional parameters in order
    pub pos: EcoVec<Interned<Pattern>>,
    /// Named parameters with their default patterns
    pub named: EcoVec<(DeclExpr, Interned<Pattern>)>,
    /// Left spread parameter (collects extra positional arguments)
    pub spread_left: Option<(DeclExpr, Interned<Pattern>)>,
    /// Right spread parameter (collects remaining arguments)
    pub spread_right: Option<(DeclExpr, Interned<Pattern>)>,
}

impl Pattern {}

impl_internable!(Decl,);

/// Represents a content sequence expression.
///
/// Used for sequences of content elements with associated type information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    /// The type of the content sequence
    pub ty: Ty,
}

/// Represents a reference expression.
///
/// Links a declaration to its usage context, including resolution steps and type information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    /// The declaration being referenced
    pub decl: DeclExpr,
    /// The intermediate step in resolution (if any)
    pub step: Option<Expr>,
    /// The root expression of the reference chain
    pub root: Option<Expr>,
    /// The final resolved type of the reference
    pub term: Option<Ty>,
}

/// Represents a content reference expression.
///
/// Used for referencing content elements like labels and citations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    /// The identifier being referenced
    pub ident: DeclExpr,
    /// The declaration this reference points to (if resolved)
    pub of: Option<DeclExpr>,
    /// The body content associated with the reference
    pub body: Option<Expr>,
}

/// Represents a field selection expression.
///
/// Used for accessing fields or methods on objects (e.g., `obj.field`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    /// The left-hand side expression being selected from
    pub lhs: Expr,
    /// The key or field name being selected
    pub key: DeclExpr,
    /// The span location of this selection
    pub span: Span,
}

impl SelectExpr {
    /// Creates a new SelectExpr with the given key and left-hand side.
    ///
    /// The span is set to detached since it's not provided.
    pub fn new(key: DeclExpr, lhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            key,
            lhs,
            span: Span::detached(),
        })
    }
}

/// Represents an arguments expression.
///
/// Contains a list of arguments and their span information for function calls.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgsExpr {
    /// The list of arguments
    pub args: Vec<ArgExpr>,
    /// The span location of the argument list
    pub span: Span,
}

impl ArgsExpr {
    /// Creates a new ArgsExpr with the given span and arguments.
    ///
    /// Wraps the arguments in an interned type for efficient sharing.
    pub fn new(span: Span, args: Vec<ArgExpr>) -> Interned<Self> {
        Interned::new(Self { args, span })
    }
}

/// Represents an element expression.
///
/// Contains an element type and its content expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementExpr {
    /// The Typst element type
    pub elem: Element,
    /// The content expressions within this element
    pub content: EcoVec<Expr>,
}

/// Represents a function application expression.
///
/// Contains the function being called, its arguments, and span information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyExpr {
    /// The function expression being called
    pub callee: Expr,
    /// The arguments passed to the function
    pub args: Expr,
    /// The span location of the function call
    pub span: Span,
}

/// Represents a function expression.
///
/// Contains the function declaration, parameter signature, and body.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    /// The declaration for this function
    pub decl: DeclExpr,
    /// The parameter signature defining function inputs
    pub params: PatternSig,
    /// The function body expression
    pub body: Expr,
}

/// Represents a let binding expression.
///
/// Contains the pattern being bound and the optional body expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    /// Span of the pattern
    pub span: Span,
    /// The pattern being bound (left side of assignment)
    pub pattern: Interned<Pattern>,
    /// The optional body expression (right side of assignment)
    pub body: Option<Expr>,
}

/// Represents a show rule expression.
///
/// Contains an optional selector and the edit function to apply.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowExpr {
    /// Optional selector expression to determine what to show
    pub selector: Option<Expr>,
    /// The edit function to apply to selected elements
    pub edit: Expr,
}

/// Represents a set rule expression.
///
/// Contains the target, arguments, and optional condition for the rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetExpr {
    /// The target element or function to set
    pub target: Expr,
    /// The arguments to apply to the target
    pub args: Expr,
    /// Optional condition for when to apply the set rule
    pub cond: Option<Expr>,
}

/// Represents an import expression.
///
/// Contains the declaration representing what is being imported.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportExpr {
    /// The reference expression for what is being imported
    pub decl: Interned<RefExpr>,
}

/// Represents an include expression.
///
/// Contains the source expression specifying what to include.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeExpr {
    /// The source expression indicating what file or content to include
    pub source: Expr,
}

/// Represents a conditional (if) expression.
///
/// Contains condition, then branch, and else branch expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfExpr {
    /// The condition expression to evaluate
    pub cond: Expr,
    /// The expression to evaluate if condition is true
    pub then: Expr,
    /// The expression to evaluate if condition is false
    pub else_: Expr,
}

/// Represents a while loop expression.
///
/// Contains the loop condition and body expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileExpr {
    /// The condition expression evaluated each iteration
    pub cond: Expr,
    /// The body expression executed while condition is true
    pub body: Expr,
}

/// Represents a for loop expression.
///
/// Contains the iteration pattern, iterable expression, and loop body.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForExpr {
    /// The pattern to match each iteration value against
    pub pattern: Interned<Pattern>,
    /// The expression that produces values to iterate over
    pub iter: Expr,
    /// The body expression executed for each iteration
    pub body: Expr,
}

/// The kind of unary operation.
///
/// Represents all possible unary operations that can be applied to expressions.
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

/// A unary operation type.
///
/// Represents the application of a unary operator to an operand.
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
    /// Create a unary operation type with the given operator and operand.
    ///
    /// Returns an interned unary operation for efficient sharing.
    pub fn new(op: UnaryOp, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }
}

impl<T> UnInst<T> {
    /// Get the operands of the unary operation.
    ///
    /// Returns an array containing a reference to the single operand.
    pub fn operands(&self) -> [&T; 1] {
        [&self.lhs]
    }
}

/// Type alias for binary operation types.
///
/// Reuses the binary operation types from the AST.
pub type BinaryOp = ast::BinOp;

/// A binary operation type.
///
/// Represents the application of a binary operator to two operands.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct BinInst<T> {
    /// The operands of the binary operation (left, right)
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
    /// Create a binary operation type with the given operator and operands.
    ///
    /// Returns an interned binary operation for efficient sharing.
    pub fn new(op: BinaryOp, lhs: Expr, rhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }
}

impl<T> BinInst<T> {
    /// Get the operands of the binary operation.
    ///
    /// Returns an array containing references to both operands (left, right).
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
