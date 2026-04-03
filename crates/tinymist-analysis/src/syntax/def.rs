//! Definitions of syntax structures.

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
    /// Creates a new [`ExprInfo`] instance from expression information
    /// representation.
    ///
    /// Wraps the provided representation in an Arc and LazyHash for efficient
    /// sharing and hashing.
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

/// Representation of [`ExprInfo`] for a specific file.
///
/// Contains all the analyzed information including resolution maps,
/// documentation strings, imports, and exports.
#[derive(Debug)]
pub struct ExprInfoRepr {
    /// The file ID this expression information belongs to.
    pub fid: TypstFileId,
    /// Revision number for tracking changes to the file.
    pub revision: usize,
    /// The source code content.
    pub source: Source,
    /// The root expression of the file.
    pub root: Expr,
    /// Documentation string for the module.
    pub module_docstring: Arc<DocString>,
    /// The lexical scope of exported symbols from this file.
    pub exports: Arc<LazyHash<LexicalScope>>,
    /// Map from file IDs to imported lexical scopes.
    pub imports: FxHashMap<TypstFileId, Arc<LazyHash<LexicalScope>>>,
    /// Map from spans to expressions for scope analysis.
    pub exprs: FxHashMap<Span, Expr>,
    /// Map from spans to resolved reference expressions.
    pub resolves: FxHashMap<Span, Interned<RefExpr>>,
    /// Map from declarations to their documentation strings.
    pub docstrings: FxHashMap<DeclExpr, Arc<DocString>>,
    /// Layout information for module import items in this file.
    pub module_items: FxHashMap<Interned<Decl>, ModuleItemLayout>,
}

impl std::hash::Hash for ExprInfoRepr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // already contained in the source.
        // self.fid.hash(state);
        self.revision.hash(state);
        self.source.hash(state);
        self.root.hash(state);
        self.exports.hash(state);
        let mut resolves = self.resolves.iter().collect::<Vec<_>>();
        resolves.sort_by_key(|(fid, _)| fid.into_raw());
        resolves.hash(state);
        let mut imports = self.imports.iter().collect::<Vec<_>>();
        imports.sort_by_key(|(fid, _)| *fid);
        imports.hash(state);
        let mut module_items = self.module_items.iter().collect::<Vec<_>>();
        module_items.sort_by_key(|(decl, _)| decl.span().into_raw());
        module_items.hash(state);
    }
}

impl ExprInfoRepr {
    /// Gets the definition expression for a given declaration.
    pub fn get_def(&self, decl: &Interned<Decl>) -> Option<Expr> {
        if decl.is_def() {
            return Some(Expr::Decl(decl.clone()));
        }
        let resolved = self.resolves.get(&decl.span())?;
        Some(Expr::Ref(resolved.clone()))
    }

    /// Gets all references to a given declaration.
    pub fn get_refs(
        &self,
        decl: Interned<Decl>,
    ) -> impl Iterator<Item = (&Span, &Interned<RefExpr>)> {
        let of = Some(Expr::Decl(decl.clone()));
        self.resolves
            .iter()
            .filter(move |(_, r)| match (decl.as_ref(), r.decl.as_ref()) {
                (Decl::Label(..), Decl::Label(..))
                | (Decl::Label(..), Decl::ContentRef(..))
                | (Decl::ContentRef(..), Decl::Label(..))
                | (Decl::ContentRef(..), Decl::ContentRef(..)) => r.decl.name() == decl.name(),
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

    /// Shows the expression information.
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

/// Describes how an import item is laid out in the source text.
#[derive(Debug, Clone, Hash)]
pub struct ModuleItemLayout {
    /// The module declaration that owns this item.
    pub parent: Interned<Decl>,
    /// The byte range covering the whole `foo as bar` clause.
    pub item_range: Range<usize>,
    /// The byte range covering the bound identifier (`bar` in `foo as bar`).
    pub binding_range: Range<usize>,
}

/// Represents different kinds of expressions in the language.
///
/// This enum covers all possible expression types that can appear in Typst
/// source code, from basic literals to complex control flow constructs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A sequence of expressions: `{ x; y; z }`
    Block(Interned<Vec<Expr>>),
    /// An array literal: `(1, 2, 3)`
    Array(Interned<ArgsExpr>),
    /// A dict literal: `(a: 1, b: 2)`
    Dict(Interned<ArgsExpr>),
    /// An args literal: `(1, 2, 3)`
    Args(Interned<ArgsExpr>),
    /// A pattern: `(x, y, ..z)`
    Pattern(Interned<Pattern>),
    /// An element literal: `[*Hi* there!]`
    Element(Interned<ElementExpr>),
    /// An unary operation: `-x`
    Unary(Interned<UnExpr>),
    /// A binary operation: `x + y`
    Binary(Interned<BinExpr>),
    /// A function call: `f(x, y)`
    Apply(Interned<ApplyExpr>),
    /// A function: `(x, y) => x + y`
    Func(Interned<FuncExpr>),
    /// A let: `let x = 1`
    Let(Interned<LetExpr>),
    /// A show: `show heading: it => emph(it.body)`
    Show(Interned<ShowExpr>),
    /// A set: `set text(...)`
    Set(Interned<SetExpr>),
    /// A reference: `#x`
    Ref(Interned<RefExpr>),
    /// A content reference: `@x`
    ContentRef(Interned<ContentRefExpr>),
    /// A select: `x.y`
    Select(Interned<SelectExpr>),
    /// An import expression: `import "path.typ": x`
    Import(Interned<ImportExpr>),
    /// An include expression: `include "path.typ"`
    Include(Interned<IncludeExpr>),
    /// A contextual expression: `context text.lang`
    Contextual(Interned<Expr>),
    /// A conditional expression: `if x { y } else { z }`
    Conditional(Interned<IfExpr>),
    /// A while loop: `while x { y }`
    WhileLoop(Interned<WhileExpr>),
    /// A for loop: `for x in y { z }`
    ForLoop(Interned<ForExpr>),
    /// A type: `str`
    Type(Ty),
    /// A declaration: `x`
    Decl(DeclExpr),
    /// A star import: `*`
    Star,
}

impl Expr {
    /// Returns a string representation of the expression.
    pub fn repr(&self) -> EcoString {
        let mut s = EcoString::new();
        let _ = ExprDescriber::new(&mut s).write_expr(self);
        s
    }

    /// Returns the span location of the expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::Decl(decl) => decl.span(),
            Expr::Select(select) => select.span,
            Expr::Apply(apply) => apply.span,
            _ => Span::detached(),
        }
    }

    /// Returns the file ID associated with this expression, if any.
    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Expr::Decl(decl) => decl.file_id(),
            _ => self.span().id(),
        }
    }

    /// Returns whether the expression is definitely defined.
    pub fn is_defined(&self) -> bool {
        match self {
            Expr::Ref(refs) => refs.root.is_some() || refs.term.is_some(),
            Expr::Decl(decl) => decl.is_def(),
            // There are unsure cases, like `x.y`, which may be defined or not.
            _ => false,
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
    /// A lexical scope extracted from a source file.
    Lexical(LexicalScope),
    /// A module instance which is either built-in or evaluated during analysis.
    Module(Module),
    /// A scope bound to a function.
    Func(Func),
    /// A scope bound to a type.
    Type(Type),
}

impl ExprScope {
    /// Creates an empty lexical scope.
    pub fn empty() -> Self {
        ExprScope::Lexical(LexicalScope::default())
    }

    /// Checks if the scope contains no bindings.
    pub fn is_empty(&self) -> bool {
        match self {
            ExprScope::Lexical(scope) => scope.is_empty(),
            ExprScope::Module(module) => is_empty_scope(module.scope()),
            ExprScope::Func(func) => func.scope().is_none_or(is_empty_scope),
            ExprScope::Type(ty) => is_empty_scope(ty.scope()),
        }
    }

    /// Looks up a name in the scope and returns both expression and type
    /// information.
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
#[derive(Debug, Default, Clone, Copy, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefKind {
    /// A definition for some constant: `let x = 1`
    #[default]
    Constant,
    /// A definition for some function: `(x, y) => x + y`
    Function,
    /// A definition for some variable: `let x = (x, y) => x + y`
    Variable,
    /// A definition for some module.
    Module,
    /// A definition for some struct (type).
    Struct,
    /// A definition for some reference: `<label>`
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
pub type DeclExpr = Interned<Decl>;

/// Represents different kinds of declarations in the language.
#[derive(Clone, PartialEq, Eq, Hash, DeclEnum)]
pub enum Decl {
    /// A function declaration: `(x, y) => x + y`
    Func(SpannedDecl),
    /// An import alias declaration: `import "path.typ": x`
    ImportAlias(SpannedDecl),
    /// A variable declaration: `let x = 1`
    Var(SpannedDecl),
    /// An identifier reference declaration: `x`
    IdentRef(SpannedDecl),
    /// A module declaration: `import calc`
    Module(ModuleDecl),
    /// A module alias declaration: `import "path.typ" as x`
    ModuleAlias(SpannedDecl),
    /// A path stem declaration: `path.typ`
    PathStem(SpannedDecl),
    /// An import path declaration: `import "path.typ"`
    ImportPath(SpannedDecl),
    /// An include path declaration: `include "path.typ"`
    IncludePath(SpannedDecl),
    /// An import declaration: `import "path.typ"`
    Import(SpannedDecl),
    /// A content reference declaration: `@x`
    ContentRef(SpannedDecl),
    /// A label declaration: `label`
    Label(SpannedDecl),
    /// A string name declaration: `"x"`
    StrName(SpannedDecl),
    /// A module import declaration: `import "path.typ": *`
    ModuleImport(SpanDecl),
    /// A closure declaration: `(x, y) => x + y`
    Closure(SpanDecl),
    /// A pattern declaration: `let (x, y, ..z) = 1`
    Pattern(SpanDecl),
    /// A spread declaration: `..z`
    Spread(SpanDecl),
    /// A content declaration: `#[text]`
    Content(SpanDecl),
    /// A constant declaration: `let x = 1`
    Constant(SpanDecl),
    /// A bib entry declaration: `@entry`
    BibEntry(NameRangeDecl),
    /// A docs declaration created by the compiler.
    Docs(DocsDecl),
    /// A generated declaration created by the compiler.
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

    /// Creates a variable declaration from a string literal.
    pub fn lit(name: &str) -> Self {
        Self::Var(SpannedDecl {
            name: name.into(),
            at: Span::detached(),
        })
    }

    /// Creates a variable declaration from an interned string.
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

    /// Creates a module declaration with a file ID.
    pub fn module(fid: TypstFileId) -> Self {
        let name = {
            let stem = fid.vpath().as_rooted_path().file_stem();
            stem.and_then(|s| Some(Interned::new_str(s.to_str()?)))
                .unwrap_or_default()
        };
        Self::Module(ModuleDecl { name, fid })
    }

    /// Creates a module declaration with a name and a file ID.
    pub fn module_with_name(name: Interned<str>, fid: TypstFileId) -> Self {
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
            at: {
                let marker_span = ident
                    .to_untyped()
                    .children()
                    .find(|child| child.kind() == SyntaxKind::RefMarker)
                    .map(|child| child.span());

                marker_span.unwrap_or(ident.span())
            },
        })
    }

    /// Creates a string name declaration from a syntax node and name.
    pub fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName(SpannedDecl {
            name: name.into(),
            at: s.span(),
        })
    }

    /// Calculates the path stem from a string path or package specification.
    ///
    /// For package specs (starting with '@'), extracts the package name.
    /// For file paths, extracts the file stem. Returns empty string if
    /// extraction fails.
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

    /// Creates an import path declaration with a span and name.
    pub fn import_path(s: Span, name: Interned<str>) -> Self {
        Self::ImportPath(SpannedDecl { name, at: s })
    }

    /// Creates an include path declaration with a span and name.
    pub fn include_path(s: Span, name: Interned<str>) -> Self {
        Self::IncludePath(SpannedDecl { name, at: s })
    }

    /// Creates a module import declaration with just a span.
    pub fn module_import(s: Span) -> Self {
        Self::ModuleImport(SpanDecl(s))
    }

    /// Creates a closure declaration with just a span.
    pub fn closure(s: Span) -> Self {
        Self::Closure(SpanDecl(s))
    }

    /// Creates a pattern declaration with just a span.
    pub fn pattern(s: Span) -> Self {
        Self::Pattern(SpanDecl(s))
    }

    /// Creates a spread declaration with just a span.
    pub fn spread(s: Span) -> Self {
        Self::Spread(SpanDecl(s))
    }

    /// Creates a content declaration with just a span.
    pub fn content(s: Span) -> Self {
        Self::Content(SpanDecl(s))
    }

    /// Creates a constant declaration with just a span.
    pub fn constant(s: Span) -> Self {
        Self::Constant(SpanDecl(s))
    }

    /// Creates a documentation declaration linking a base declaration with a
    /// type variable.
    pub fn docs(base: Interned<Decl>, var: Interned<TypeVar>) -> Self {
        Self::Docs(DocsDecl { base, var })
    }

    /// Creates a generated declaration with a definition ID.
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

    /// Checks if this declaration represents a definition rather than a
    /// reference (usage of a definition).
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

    /// Gets full range of the declaration.
    pub fn full_range(&self) -> Option<Range<usize>> {
        if let Decl::BibEntry(decl) = self {
            return decl.at.2.clone();
        }

        None
    }

    /// Creates a reference expression that points to this declaration.
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
    /// Low-performance comparison that is free from concurrency issues.
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

/// A declaration with an associated name and span location.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpannedDecl {
    /// The name of the declaration.
    name: Interned<str>,
    /// The span location of the declaration.
    at: Span,
}

impl SpannedDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    /// Gets the span location of the declaration.
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
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NameRangeDecl {
    /// The name of the declaration.
    pub name: Interned<str>,
    /// Boxed tuple containing (file_id, name_range, full_range).
    pub at: Box<(TypstFileId, Range<usize>, Option<Range<usize>>)>,
}

impl NameRangeDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    /// Gets the span location of the declaration.
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
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ModuleDecl {
    /// The name of the module.
    pub name: Interned<str>,
    /// The file ID where the module is defined.
    pub fid: TypstFileId,
}

impl ModuleDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        &self.name
    }

    /// Gets the span location of the declaration.
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
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocsDecl {
    base: Interned<Decl>,
    var: Interned<TypeVar>,
}

impl DocsDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    /// Gets the span location of the declaration.
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
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpanDecl(Span);

impl SpanDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    /// Gets the span location of the declaration.
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
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GeneratedDecl(DefId);

impl GeneratedDecl {
    /// Gets the name of the declaration.
    fn name(&self) -> &Interned<str> {
        Interned::empty()
    }

    /// Gets the span location of the declaration.
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
    /// A positional argument: `x`
    Pos(Expr),
    /// A named argument: `a: x`
    Named(Box<(DeclExpr, Expr)>),
    /// A named argument with a default value: `((a): x)`
    NamedRt(Box<(Expr, Expr)>),
    /// A spread argument: `..x`
    Spread(Expr),
}

/// Represents different kinds of patterns for destructuring.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// A general pattern expression can occur in right-hand side of a
    /// function signature.
    Expr(Expr),
    /// A simple pattern: `x`
    Simple(Interned<Decl>),
    /// A pattern signature: `(x, y: val, ..z)`
    Sig(Box<PatternSig>),
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        ExprPrinter::new(f).write_pattern(self)
    }
}

impl Pattern {
    /// Returns a string representation of the pattern.
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
    /// Positional parameters in order.
    pub pos: EcoVec<Interned<Pattern>>,
    /// Named parameters with their default patterns.
    pub named: EcoVec<(DeclExpr, Interned<Pattern>)>,
    /// Left spread parameter (collects extra positional arguments).
    pub spread_left: Option<(DeclExpr, Interned<Pattern>)>,
    /// Right spread parameter (collects remaining arguments).
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
/// A reference expression tracks how an identifier resolves through the lexical
/// scope, imports, and field accesses. It maintains a chain of resolution steps
/// to support features like go-to-definition, go-to-reference, and type
/// inference.
///
/// # Resolution Chain
///
/// The fields form a resolution chain: `root` -> `step` -> `decl`, where:
/// - `root` is the original source of the value
/// - `step` is any intermediate transformation
/// - `decl` is the final identifier being referenced
/// - `term` is the resolved type (if known)
///   - Hint: A value `1`'s typst type is `int`, but here we keep the type as
///     `1` to improve the type inference.
///
/// # Examples
///
/// ## Simple identifier reference
/// ```rust,ignore
/// // For: let x = value; let y = x;
/// RefExpr {
///     decl: y,           // The identifier 'y'
///     root: Some(x),     // Points back to 'x'
///     step: Some(x),     // Same as root for simple refs
///     term: None,        // Type may not be known yet
/// }
/// ```
///
/// ## Import with rename
/// ```rust,ignore
/// // For: import "mod.typ": old as new
/// // First creates ref for 'old':
/// RefExpr { decl: old, root: Some(mod.old), step: Some(field), term: Some(Func(() -> dict)) }
/// // Then creates ref for 'new':
/// RefExpr { decl: new, root: Some(mod.old), step: Some(old), term: Some(Func(() -> dict)) }
/// ```
///
/// ## Builtin definitions
/// ```rust,ignore
/// // For: std.length
/// RefExpr { decl: length, root: None, step: None, term: Some(Type(length)) }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    /// The declaration being referenced (the final identifier in the chain).
    ///
    /// This is always set and represents the identifier at the current point
    /// of reference (e.g., the variable name, import alias, or field name).
    pub decl: DeclExpr,

    /// The intermediate expression in the resolution chain.
    ///
    /// Set in the following cases:
    /// - **Import/include**: The module expression being imported
    /// - **Field access**: The selected field's expression
    /// - **Scope resolution**: The scope expression being resolved
    /// - **Renamed imports**: The original name before renaming
    ///
    /// `None` when the identifier is an undefined reference.
    pub step: Option<Expr>,

    /// The root expression at the start of the reference chain.
    ///
    /// A root definition never references another root definition.
    pub root: Option<Expr>,

    /// The final resolved type of the referenced value.
    ///
    /// Set whenever a type is known for the referenced value.
    ///
    /// Some reference doesn't have a root definition, but has a term. For
    /// example, `std.length` is termed as `Type(length)` while has no a
    /// definition.
    pub term: Option<Ty>,
}

/// Represents a content reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    /// The identifier being referenced.
    pub ident: DeclExpr,
    /// The declaration this reference points to (if resolved).
    pub of: Option<DeclExpr>,
    /// The body content associated with the reference.
    pub body: Option<Expr>,
}

/// Represents a field selection expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    /// The left-hand side expression being selected from.
    pub lhs: Expr,
    /// The key or field name being selected.
    pub key: DeclExpr,
    /// The span location of this selection.
    pub span: Span,
}

impl SelectExpr {
    /// Creates a new SelectExpr with the given key and left-hand side.
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
    /// The list of arguments.
    pub args: Vec<ArgExpr>,
    /// The span location of the argument list.
    pub span: Span,
}

impl ArgsExpr {
    /// Creates a new ArgsExpr with the given span and arguments.
    pub fn new(span: Span, args: Vec<ArgExpr>) -> Interned<Self> {
        Interned::new(Self { args, span })
    }
}

/// Represents an element expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementExpr {
    /// The Typst element type.
    pub elem: Element,
    /// The content expressions within this element.
    pub content: EcoVec<Expr>,
}

/// Represents a function application expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyExpr {
    /// The function expression being called.
    pub callee: Expr,
    /// The arguments passed to the function.
    pub args: Expr,
    /// The span location of the function call.
    pub span: Span,
}

/// Represents a function expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    /// The declaration for this function.
    pub decl: DeclExpr,
    /// The parameter signature defining function inputs.
    pub params: PatternSig,
    /// The function body expression.
    pub body: Expr,
}

/// Represents a let binding expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    /// Span of the pattern.
    pub span: Span,
    /// The pattern being bound (left side of assignment).
    pub pattern: Interned<Pattern>,
    /// The optional body expression (right side of assignment).
    pub body: Option<Expr>,
}

/// Represents a show rule expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowExpr {
    /// Optional selector expression to determine what to show.
    pub selector: Option<Expr>,
    /// The edit function to apply to selected elements.
    pub edit: Expr,
}

/// Represents a set rule expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetExpr {
    /// The target element or function to set.
    pub target: Expr,
    /// The arguments to apply to the target.
    pub args: Expr,
    /// Optional condition for when to apply the set rule.
    pub cond: Option<Expr>,
}

/// Represents an import expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportExpr {
    /// The source expression indicating what file or module to import from.
    pub source: Expr,
    /// The reference expression for what is being imported.
    pub decl: Interned<RefExpr>,
}

/// Represents an include expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeExpr {
    /// The source expression indicating what file or content to include.
    pub source: Expr,
}

/// Represents a conditional (if) expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfExpr {
    /// The condition expression to evaluate.
    pub cond: Expr,
    /// The expression to evaluate if condition is true.
    pub then: Expr,
    /// The expression to evaluate if condition is false.
    pub else_: Expr,
}

/// Represents a while loop expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileExpr {
    /// The condition expression evaluated each iteration.
    pub cond: Expr,
    /// The body expression executed while condition is true.
    pub body: Expr,
}

/// Represents a for loop expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForExpr {
    /// The pattern to match each iteration value against.
    pub pattern: Interned<Pattern>,
    /// The expression that produces values to iterate over.
    pub iter: Expr,
    /// The body expression executed for each iteration.
    pub body: Expr,
}

/// The kind of unary operation.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum UnaryOp {
    /// The (arithmetic) positive operation.
    /// `+t`
    Pos,
    /// The (arithmetic) negate operation.
    /// `-t`
    Neg,
    /// The (logical) not operation.
    /// `not t`
    Not,
    /// The return operation.
    /// `return t`
    Return,
    /// The typst context operation.
    /// `context t`
    Context,
    /// The spreading operation.
    /// `..t`
    Spread,
    /// The not element of operation.
    /// `not in t`
    NotElementOf,
    /// The element of operation.
    /// `in t`
    ElementOf,
    /// The type of operation.
    /// `type(t)`
    TypeOf,
}

/// A unary operation type.
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
    /// Creates a unary operation type with the given operator and operand.
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

/// Type alias for binary operation types.
pub type BinaryOp = ast::BinOp;

/// A binary operation type.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct BinInst<T> {
    /// The operands of the binary operation (left, right).
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
    /// Creates a binary operation type with the given operator and operands.
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

/// Checks if a scope is empty.
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
