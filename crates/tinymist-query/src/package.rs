//! Package management tools.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::{self, Write as _};
use std::ops::Range;
use std::path::PathBuf;

use ecow::eco_format;
#[cfg(feature = "local-registry")]
use ecow::{EcoVec, eco_vec};
// use reflexo_typst::typst::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use tinymist_world::package::registry::PackageIndexEntry;
use tinymist_world::package::{PackageSpec, PackageSpecExt};
use typst::World;
use typst::diag::{EcoString, StrResult};
use typst::syntax::package::PackageManifest;
use typst::syntax::{
    FileId, LinkedNode, RootedPath, Source, Span, SyntaxKind, VirtualPath, VirtualRoot, ast,
};
use typst_shim::syntax::{resolve_path_from_id, source_range};

use crate::LocalContext;
use crate::analysis::{SharedContext, TypeInfo};
use crate::syntax::{DeclExpr, Expr, LexicalScope, Pattern, PatternSig};
use crate::ty::Ty;

/// Information about a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// The path to the package if any.
    pub path: PathBuf,
    /// The namespace the package lives in.
    pub namespace: EcoString,
    /// The name of the package within its namespace.
    pub name: EcoString,
    /// The package's version.
    pub version: String,
}

impl From<PackageIndexEntry> for PackageInfo {
    fn from(entry: PackageIndexEntry) -> Self {
        let spec = entry.spec();
        Self {
            path: entry.path.unwrap_or_default(),
            namespace: spec.namespace,
            name: spec.name,
            version: spec.version.to_string(),
        }
    }
}

/// Parses a package import from a string literal node in an import statement.
/// Returns the PackageSpec if it's a valid package import.
pub fn parse_package_import(node: &LinkedNode) -> Option<PackageSpec> {
    if !matches!(node.kind(), SyntaxKind::Str) {
        return None;
    }

    let import_node = node.parent()?.cast::<ast::ModuleImport>()?;

    let ast::Expr::Str(str_node) = import_node.source() else {
        return None;
    };
    let import_str = str_node.get();
    if import_str.starts_with('@') {
        import_str.parse().ok()
    } else {
        None
    }
}

/// Finds the package entry for a given package spec, and also the latest
/// version entry.
pub fn find_package_and_latest<'a>(
    ctx: &'a SharedContext,
    package_spec: &PackageSpec,
) -> (
    Option<Cow<'a, PackageIndexEntry>>,
    Option<Cow<'a, PackageIndexEntry>>,
) {
    let versionless_spec = package_spec.versionless();

    if package_spec.is_preview() {
        let packages = ctx.world().packages();

        let current = packages.iter().find(|it| it.matches(package_spec));
        let latest = packages
            .iter()
            .filter(|it| it.matches_versionless(&versionless_spec))
            .max_by_key(|entry| entry.package.version);

        (current.map(Cow::Borrowed), latest.map(Cow::Borrowed))
    } else if cfg!(feature = "local-registry") {
        let local_packages = ctx.non_preview_packages();

        let current = local_packages.iter().find(|it| it.matches(package_spec));
        let latest = local_packages
            .iter()
            .filter(|it| it.matches_versionless(&versionless_spec))
            .max_by_key(|entry| entry.package.version);

        (
            current.map(|p| Cow::Owned(p.clone())),
            latest.map(|p| Cow::Owned(p.clone())),
        )
    } else {
        (None, None)
    }
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest_id(spec: &PackageInfo) -> StrResult<FileId> {
    Ok(FileId::new(RootedPath::new(
        VirtualRoot::Package(PackageSpec {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.parse()?,
        }),
        VirtualPath::new("typst.toml").expect("valid manifest path"),
    )))
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest(world: &dyn World, toml_id: FileId) -> StrResult<PackageManifest> {
    let toml_data = world
        .file(toml_id)
        .map_err(|err| eco_format!("failed to read package manifest ({})", err))?;

    let string = std::str::from_utf8(&toml_data)
        .map_err(|err| eco_format!("package manifest is not valid UTF-8 ({})", err))?;

    toml::from_str(string)
        .map_err(|err| eco_format!("package manifest is malformed ({})", err.message()))
}

pub(crate) fn package_entrypoint_id(manifest_id: FileId, entrypoint: &str) -> FileId {
    resolve_path_from_id(manifest_id, entrypoint)
        .expect("valid package entrypoint")
        .intern()
}

/// Check Package.
pub fn check_package(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<()> {
    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let entry_point = package_entrypoint_id(toml_id, &manifest.package.entrypoint);

    ctx.preload_package(entry_point);
    Ok(())
}

/// Dumps package scopes together with type-checker results.
pub fn package_tyck_scope(
    ctx: &mut LocalContext,
    spec: &PackageInfo,
    options: PackageTyckDumpOptions,
) -> StrResult<PackageTyckDump> {
    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;
    let entry_point = package_entrypoint_id(toml_id, &manifest.package.entrypoint);
    let files = collect_package_tyck_files(ctx, entry_point, options)?;

    Ok(PackageTyckDump {
        schema: 1,
        package: DumpPackageInfo {
            namespace: spec.namespace.to_string(),
            name: spec.name.to_string(),
            version: spec.version.clone(),
            spec: format!("@{}/{}:{}", spec.namespace, spec.name, spec.version),
            path: spec.path.to_string_lossy().into_owned(),
            entrypoint: manifest.package.entrypoint.to_string(),
        },
        entrypoint: dump_file_id(entry_point),
        files,
    })
}

fn collect_package_tyck_files(
    ctx: &mut LocalContext,
    entry_point: FileId,
    options: PackageTyckDumpOptions,
) -> StrResult<Vec<DumpFile>> {
    let package_root = entry_point.root().clone();
    let mut files = vec![];
    let mut seen = FxHashSet::default();
    let mut queue = VecDeque::from([entry_point]);

    while let Some(fid) = queue.pop_front() {
        if !seen.insert(fid) {
            continue;
        }

        let source = match ctx.source_by_id(fid) {
            Ok(source) => source,
            Err(err) if fid == entry_point => {
                return Err(eco_format!(
                    "failed to read package entrypoint {fid:?}: {err}"
                ));
            }
            Err(err) => {
                log::warn!("skipping unreadable package source {fid:?}: {err}");
                continue;
            }
        };
        let expr_info = ctx.expr_stage(&source);
        let type_info = ctx.type_check(&source);

        let mut imported_files = expr_info
            .imports
            .keys()
            .copied()
            .map(dump_file_id)
            .collect::<Vec<_>>();
        imported_files.sort_by(|left, right| left.file_id.cmp(&right.file_id));

        for imported in expr_info.imports.keys().copied() {
            if imported.root() == &package_root && !seen.contains(&imported) {
                queue.push_back(imported);
            }
        }

        files.push(dump_file_scope(
            &source,
            &expr_info.exports,
            &expr_info.root,
            &type_info,
            imported_files,
            options,
        ));
    }

    files.sort_by(|left, right| left.file_id.cmp(&right.file_id));
    Ok(files)
}

/// Options for dumping package scope and type-checker information.
#[derive(Debug, Clone, Copy, Default)]
pub struct PackageTyckDumpOptions {
    /// Maximum characters kept for each dumped type string.
    ///
    /// Set to `None` to keep full type strings. Very large inferred types can
    /// otherwise make the JSON dump impractical for downstream scripts.
    pub max_type_chars: Option<usize>,
}

/// Package scope and type-checker dump.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageTyckDump {
    schema: u32,
    package: DumpPackageInfo,
    entrypoint: DumpFileId,
    files: Vec<DumpFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpPackageInfo {
    namespace: String,
    name: String,
    version: String,
    spec: String,
    path: String,
    entrypoint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpFileId {
    file_id: String,
    root: String,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpFile {
    file_id: String,
    root: String,
    path: String,
    imports: Vec<DumpFileId>,
    scopes: Vec<DumpScope>,
    type_mappings: Vec<DumpTypeMapping>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpScope {
    kind: &'static str,
    name: String,
    declaration: Option<DumpDecl>,
    variables: Vec<DumpVariable>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpVariable {
    name: String,
    kind: String,
    source: &'static str,
    exported: bool,
    declaration: DumpDecl,
    expression: Option<String>,
    ty: Option<DumpType>,
}

#[derive(Debug, Clone, Copy)]
struct DumpVariableOrigin {
    source: &'static str,
    exported: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpDecl {
    debug: String,
    kind: String,
    file_id: Option<String>,
    range: Option<DumpRange>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpType {
    debug: EcoString,
    describe: Option<EcoString>,
    repr: Option<EcoString>,
}

struct TypeDumper<'a> {
    type_info: &'a TypeInfo,
    options: PackageTyckDumpOptions,
    cache: FxHashMap<Ty, DumpType>,
}

impl<'a> TypeDumper<'a> {
    fn new(type_info: &'a TypeInfo, options: PackageTyckDumpOptions) -> Self {
        Self {
            type_info,
            options,
            cache: FxHashMap::default(),
        }
    }

    fn dump(&mut self, source: Ty) -> DumpType {
        if let Some(dump) = self.cache.get(&source) {
            return dump.clone();
        }

        let ty = self.type_info.simplify(source.clone(), true);
        let display_ty = if contains_signature_binders(&ty) {
            self.type_info.simplify(source.clone(), false)
        } else {
            ty.clone()
        };
        let dump = DumpType {
            debug: format_debug_dump(&ty, self.options.max_type_chars).into(),
            describe: display_ty.describe().map(|text| {
                truncate_dump_string(text.to_string(), self.options.max_type_chars).into()
            }),
            repr: display_ty.repr().map(|text| {
                truncate_dump_string(text.to_string(), self.options.max_type_chars).into()
            }),
        };
        self.cache.insert(source, dump.clone());
        dump
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpTypeMapping {
    range: DumpRange,
    ty: DumpType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpRange {
    start: usize,
    end: usize,
}

fn dump_file_scope(
    source: &Source,
    exports: &LexicalScope,
    root_expr: &Expr,
    type_info: &TypeInfo,
    imports: Vec<DumpFileId>,
    options: PackageTyckDumpOptions,
) -> DumpFile {
    let fid = source.id();
    let file = dump_file_id(fid);
    let mut types = TypeDumper::new(type_info, options);

    let file_scope = DumpScope {
        kind: "file",
        name: file.path.clone(),
        declaration: None,
        variables: dump_scope_variables(source, &mut types, exports, "export", true),
    };

    let mut scopes = vec![file_scope];
    collect_function_scopes(source, &mut types, root_expr, &mut scopes);

    DumpFile {
        file_id: file.file_id,
        root: file.root,
        path: file.path,
        imports,
        scopes,
        type_mappings: dump_type_mappings(source, &mut types),
    }
}

fn dump_scope_variables(
    source: &Source,
    types: &mut TypeDumper,
    scope: &LexicalScope,
    var_source: &'static str,
    exported: bool,
) -> Vec<DumpVariable> {
    let origin = DumpVariableOrigin {
        source: var_source,
        exported,
    };
    let mut vars = scope
        .iter()
        .filter_map(|(name, expr)| {
            let decl = expr_decl(expr)?;
            Some(dump_variable(
                source,
                types,
                name.as_ref(),
                decl,
                Some(expr),
                origin,
            ))
        })
        .collect::<Vec<_>>();

    vars.sort_by(variable_cmp);
    vars.dedup_by(|left, right| {
        left.name == right.name && left.declaration.debug == right.declaration.debug
    });
    vars
}

fn collect_function_scopes(
    source: &Source,
    types: &mut TypeDumper,
    expr: &Expr,
    scopes: &mut Vec<DumpScope>,
) {
    if let Expr::Func(func) = expr {
        let mut variables = vec![];
        collect_pattern_sig_variables(source, types, &func.params, "parameter", &mut variables);
        collect_local_variables(source, types, &func.body, &mut variables);
        variables.sort_by(variable_cmp);
        variables.dedup_by(|left, right| {
            left.name == right.name && left.declaration.debug == right.declaration.debug
        });

        scopes.push(DumpScope {
            kind: "function",
            name: scope_name(&func.decl),
            declaration: Some(dump_decl(source, &func.decl)),
            variables,
        });

        collect_function_scopes(source, types, &func.body, scopes);
        return;
    }

    walk_expr_children(expr, &mut |child| {
        collect_function_scopes(source, types, child, scopes);
    });
}

fn collect_local_variables(
    source: &Source,
    types: &mut TypeDumper,
    expr: &Expr,
    variables: &mut Vec<DumpVariable>,
) {
    match expr {
        Expr::Func(_) => {}
        Expr::Let(let_expr) => {
            collect_pattern_variables(source, types, &let_expr.pattern, "local", variables);
            if let Some(body) = &let_expr.body {
                collect_local_variables(source, types, body, variables);
            }
        }
        Expr::ForLoop(for_loop) => {
            collect_pattern_variables(source, types, &for_loop.pattern, "local", variables);
            collect_local_variables(source, types, &for_loop.iter, variables);
            collect_local_variables(source, types, &for_loop.body, variables);
        }
        _ => {
            walk_expr_children(expr, &mut |child| {
                collect_local_variables(source, types, child, variables);
            });
        }
    }
}

fn collect_pattern_sig_variables(
    source: &Source,
    types: &mut TypeDumper,
    sig: &PatternSig,
    var_source: &'static str,
    variables: &mut Vec<DumpVariable>,
) {
    for pattern in &sig.pos {
        collect_pattern_variables(source, types, pattern, var_source, variables);
    }
    for (decl, pattern) in &sig.named {
        variables.push(dump_variable(
            source,
            types,
            decl.name().as_ref(),
            decl,
            None,
            DumpVariableOrigin {
                source: var_source,
                exported: false,
            },
        ));
        collect_pattern_variables(source, types, pattern, var_source, variables);
    }
    for (decl, pattern) in sig.spread_left.iter().chain(sig.spread_right.iter()) {
        variables.push(dump_variable(
            source,
            types,
            decl.name().as_ref(),
            decl,
            None,
            DumpVariableOrigin {
                source: var_source,
                exported: false,
            },
        ));
        collect_pattern_variables(source, types, pattern, var_source, variables);
    }
}

fn collect_pattern_variables(
    source: &Source,
    types: &mut TypeDumper,
    pattern: &Pattern,
    var_source: &'static str,
    variables: &mut Vec<DumpVariable>,
) {
    match pattern {
        Pattern::Expr(expr) => collect_local_variables(source, types, expr, variables),
        Pattern::Simple(decl) => {
            variables.push(dump_variable(
                source,
                types,
                decl.name().as_ref(),
                decl,
                None,
                DumpVariableOrigin {
                    source: var_source,
                    exported: false,
                },
            ));
        }
        Pattern::Sig(sig) => {
            collect_pattern_sig_variables(source, types, sig, var_source, variables);
        }
    }
}

fn walk_expr_children(expr: &Expr, f: &mut impl FnMut(&Expr)) {
    match expr {
        Expr::Block(exprs) => exprs.iter().for_each(f),
        Expr::Array(args) | Expr::Dict(args) | Expr::Args(args) => {
            walk_args(args.args.iter(), f);
        }
        Expr::Pattern(pattern) => walk_pattern(pattern, f),
        Expr::Element(elem) => elem.content.iter().for_each(f),
        Expr::Unary(unary) => f(&unary.lhs),
        Expr::Binary(binary) => {
            f(&binary.operands.0);
            f(&binary.operands.1);
        }
        Expr::Apply(apply) => {
            f(&apply.callee);
            f(&apply.args);
        }
        Expr::Func(func) => {
            walk_pattern_sig(&func.params, f);
            f(&func.body);
        }
        Expr::Let(let_expr) => {
            walk_pattern(&let_expr.pattern, f);
            if let Some(body) = &let_expr.body {
                f(body);
            }
        }
        Expr::Show(show) => {
            if let Some(selector) = &show.selector {
                f(selector);
            }
            f(&show.edit);
        }
        Expr::Set(set) => {
            f(&set.target);
            f(&set.args);
            if let Some(cond) = &set.cond {
                f(cond);
            }
        }
        Expr::Ref(ref_expr) => {
            if let Some(step) = &ref_expr.step {
                f(step);
            }
            if let Some(root) = &ref_expr.root {
                f(root);
            }
        }
        Expr::ContentRef(content_ref) => {
            if let Some(body) = &content_ref.body {
                f(body);
            }
        }
        Expr::Select(select) => f(&select.lhs),
        Expr::Import(import) => {
            f(&import.source);
        }
        Expr::Include(include) => {
            f(&include.source);
        }
        Expr::Contextual(inner) => f(inner),
        Expr::Conditional(cond) => {
            f(&cond.cond);
            f(&cond.then);
            f(&cond.else_);
        }
        Expr::WhileLoop(while_loop) => {
            f(&while_loop.cond);
            f(&while_loop.body);
        }
        Expr::ForLoop(for_loop) => {
            walk_pattern(&for_loop.pattern, f);
            f(&for_loop.iter);
            f(&for_loop.body);
        }
        Expr::Type(_) | Expr::Decl(_) | Expr::Star => {}
    }
}

fn walk_args<'a>(
    args: impl Iterator<Item = &'a crate::syntax::ArgExpr>,
    f: &mut impl FnMut(&Expr),
) {
    for arg in args {
        match arg {
            crate::syntax::ArgExpr::Pos(expr) | crate::syntax::ArgExpr::Spread(expr) => f(expr),
            crate::syntax::ArgExpr::Named(pair) => f(&pair.1),
            crate::syntax::ArgExpr::NamedRt(pair) => {
                f(&pair.0);
                f(&pair.1);
            }
        }
    }
}

fn walk_pattern(pattern: &Pattern, f: &mut impl FnMut(&Expr)) {
    match pattern {
        Pattern::Expr(expr) => f(expr),
        Pattern::Simple(_) => {}
        Pattern::Sig(sig) => walk_pattern_sig(sig, f),
    }
}

fn walk_pattern_sig(sig: &PatternSig, f: &mut impl FnMut(&Expr)) {
    for pattern in &sig.pos {
        walk_pattern(pattern, f);
    }
    for (_, pattern) in &sig.named {
        walk_pattern(pattern, f);
    }
    for (_, pattern) in sig.spread_left.iter().chain(sig.spread_right.iter()) {
        walk_pattern(pattern, f);
    }
}

fn expr_decl(expr: &Expr) -> Option<&DeclExpr> {
    match expr {
        Expr::Decl(decl) => Some(decl),
        Expr::Ref(ref_expr) => ref_expr
            .root
            .as_ref()
            .and_then(expr_decl)
            .or(Some(&ref_expr.decl)),
        _ => None,
    }
}

fn dump_variable(
    source: &Source,
    types: &mut TypeDumper,
    name: &str,
    decl: &DeclExpr,
    expr: Option<&Expr>,
    origin: DumpVariableOrigin,
) -> DumpVariable {
    let ty = types
        .type_info
        .vars
        .get(decl)
        .map(|bounds| bounds.as_type())
        .map(|ty| types.dump(ty));

    DumpVariable {
        name: name.to_owned(),
        kind: decl.kind().to_string(),
        source: origin.source,
        exported: origin.exported,
        declaration: dump_decl(source, decl),
        expression: expr.map(ToString::to_string),
        ty,
    }
}

fn dump_decl(source: &Source, decl: &DeclExpr) -> DumpDecl {
    DumpDecl {
        debug: format!("{decl:?}"),
        kind: decl.kind().to_string(),
        file_id: decl.file_id().map(|fid| dump_file_id(fid).file_id),
        range: dump_span_range(source, decl.span()),
    }
}

fn contains_signature_binders(ty: &Ty) -> bool {
    contains_signature_binders_inner(ty, &mut FxHashSet::default(), &mut FxHashSet::default())
}

#[allow(clippy::mutable_key_type)]
fn contains_signature_binders_inner(
    ty: &Ty,
    traversed: &mut FxHashSet<Ty>,
    type_var_traversed: &mut FxHashSet<Ty>,
) -> bool {
    if !traversed.insert(ty.clone()) {
        return false;
    }

    match ty {
        Ty::Func(sig) | Ty::Pattern(sig) => {
            sig.inputs()
                .any(|ty| contains_type_var(ty, type_var_traversed))
                || sig
                    .inputs()
                    .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed))
                || sig.body.as_ref().is_some_and(|ty| {
                    contains_signature_binders_inner(ty, traversed, type_var_traversed)
                })
        }
        Ty::Args(sig) => {
            sig.inputs()
                .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed))
                || sig.body.as_ref().is_some_and(|ty| {
                    contains_signature_binders_inner(ty, traversed, type_var_traversed)
                })
        }
        Ty::With(with) => {
            contains_signature_binders_inner(&with.sig, traversed, type_var_traversed)
                || with
                    .with
                    .inputs()
                    .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed))
                || with.with.body.as_ref().is_some_and(|ty| {
                    contains_signature_binders_inner(ty, traversed, type_var_traversed)
                })
        }
        Ty::Param(param) => {
            contains_signature_binders_inner(&param.ty, traversed, type_var_traversed)
        }
        Ty::Union(types) | Ty::Tuple(types) => types
            .iter()
            .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed)),
        Ty::Let(bounds) => bounds
            .lbs
            .iter()
            .chain(&bounds.ubs)
            .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed)),
        Ty::Dict(record) => record
            .types
            .iter()
            .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed)),
        Ty::Array(elem) => contains_signature_binders_inner(elem, traversed, type_var_traversed),
        Ty::Select(select) => {
            contains_signature_binders_inner(&select.ty, traversed, type_var_traversed)
        }
        Ty::Unary(unary) => {
            contains_signature_binders_inner(&unary.lhs, traversed, type_var_traversed)
        }
        Ty::Binary(binary) => binary
            .operands()
            .iter()
            .any(|ty| contains_signature_binders_inner(ty, traversed, type_var_traversed)),
        Ty::If(if_ty) => {
            contains_signature_binders_inner(&if_ty.cond, traversed, type_var_traversed)
                || contains_signature_binders_inner(&if_ty.then, traversed, type_var_traversed)
                || contains_signature_binders_inner(&if_ty.else_, traversed, type_var_traversed)
        }
        Ty::Var(_) | Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
    }
}

#[allow(clippy::mutable_key_type)]
fn contains_type_var(ty: &Ty, traversed: &mut FxHashSet<Ty>) -> bool {
    if !traversed.insert(ty.clone()) {
        return false;
    }

    match ty {
        Ty::Var(_) => true,
        Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
            sig.inputs().any(|ty| contains_type_var(ty, traversed))
                || sig
                    .body
                    .as_ref()
                    .is_some_and(|ty| contains_type_var(ty, traversed))
        }
        Ty::With(with) => {
            contains_type_var(&with.sig, traversed)
                || with
                    .with
                    .inputs()
                    .any(|ty| contains_type_var(ty, traversed))
                || with
                    .with
                    .body
                    .as_ref()
                    .is_some_and(|ty| contains_type_var(ty, traversed))
        }
        Ty::Param(param) => contains_type_var(&param.ty, traversed),
        Ty::Union(types) | Ty::Tuple(types) => {
            types.iter().any(|ty| contains_type_var(ty, traversed))
        }
        Ty::Let(bounds) => bounds
            .lbs
            .iter()
            .chain(&bounds.ubs)
            .any(|ty| contains_type_var(ty, traversed)),
        Ty::Dict(record) => record
            .types
            .iter()
            .any(|ty| contains_type_var(ty, traversed)),
        Ty::Array(elem) => contains_type_var(elem, traversed),
        Ty::Select(select) => contains_type_var(&select.ty, traversed),
        Ty::Unary(unary) => contains_type_var(&unary.lhs, traversed),
        Ty::Binary(binary) => binary
            .operands()
            .iter()
            .any(|ty| contains_type_var(ty, traversed)),
        Ty::If(if_ty) => {
            contains_type_var(&if_ty.cond, traversed)
                || contains_type_var(&if_ty.then, traversed)
                || contains_type_var(&if_ty.else_, traversed)
        }
        Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
    }
}

fn dump_type_mappings(source: &Source, types: &mut TypeDumper) -> Vec<DumpTypeMapping> {
    let type_info = types.type_info;
    let mut mappings = type_info
        .mapping
        .iter()
        .filter_map(|(span, mapped_types)| {
            let range = dump_span_range(source, *span)?;
            let ty = Ty::from_types(mapped_types.clone().into_iter());
            Some(DumpTypeMapping {
                range,
                ty: types.dump(ty),
            })
        })
        .collect::<Vec<_>>();
    mappings.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| left.range.end.cmp(&right.range.end))
    });
    mappings
}

struct TruncatingString {
    text: String,
    remaining_chars: usize,
    truncated: bool,
}

impl fmt::Write for TruncatingString {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.remaining_chars == 0 {
            self.truncated = true;
            return Err(fmt::Error);
        }

        if let Some((byte_idx, _)) = text.char_indices().nth(self.remaining_chars) {
            self.text.push_str(&text[..byte_idx]);
            self.remaining_chars = 0;
            self.truncated = true;
            Err(fmt::Error)
        } else {
            self.remaining_chars -= text.chars().count();
            self.text.push_str(text);
            Ok(())
        }
    }
}

fn format_debug_dump<T: fmt::Debug>(value: &T, max_chars: Option<usize>) -> String {
    let Some(max_chars) = max_chars else {
        return format!("{value:#?}");
    };

    let mut out = TruncatingString {
        text: String::new(),
        remaining_chars: max_chars,
        truncated: false,
    };
    let _ = write!(&mut out, "{value:#?}");
    if out.truncated {
        out.text.push_str(" ... truncated ...");
        out.text.shrink_to_fit();
    }
    out.text
}

fn truncate_dump_string(mut text: String, max_chars: Option<usize>) -> String {
    let Some(max_chars) = max_chars else {
        return text;
    };
    if let Some((byte_idx, _)) = text.char_indices().nth(max_chars) {
        text.truncate(byte_idx);
        text.push_str(" ... truncated ...");
        text.shrink_to_fit();
    }
    text
}

fn dump_span_range(source: &Source, span: Span) -> Option<DumpRange> {
    if span.id()? != source.id() {
        return None;
    }

    let Range { start, end } = source_range(source, span)?;
    Some(DumpRange { start, end })
}

fn dump_file_id(fid: FileId) -> DumpFileId {
    let root = match fid.root() {
        VirtualRoot::Project => "$project".to_owned(),
        VirtualRoot::Package(spec) => spec.to_string(),
    };
    let path = fid.vpath().get_without_slash().to_owned();
    let file_id = if matches!(fid.root(), VirtualRoot::Package(_)) {
        format!("{root}/{}", fid.vpath().get_without_slash())
    } else {
        fid.vpath().get_with_slash().to_owned()
    };

    DumpFileId {
        file_id,
        root,
        path,
    }
}

fn scope_name(decl: &DeclExpr) -> String {
    let name = decl.name().as_ref();
    if name.is_empty() {
        format!("{decl:?}")
    } else {
        name.to_owned()
    }
}

fn variable_cmp(left: &DumpVariable, right: &DumpVariable) -> std::cmp::Ordering {
    left.declaration
        .range
        .as_ref()
        .map(|range| (range.start, range.end))
        .cmp(
            &right
                .declaration
                .range
                .as_ref()
                .map(|range| (range.start, range.end)),
        )
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.kind.cmp(&right.kind))
}

/// A filter for packages.
#[cfg(feature = "local-registry")]
pub enum PackageFilter {
    /// Filter for packages that match the given namespace.
    For(EcoString),
    /// Filter for packages that do not match the given namespace.
    ExceptFor(EcoString),
    /// Filter that matches all packages.
    All,
}

#[cfg(feature = "local-registry")]
/// Get the packages in namespaces and their descriptions.
pub fn list_package(
    world: &tinymist_project::LspWorld,
    filter: PackageFilter,
) -> EcoVec<PackageIndexEntry> {
    trait IsDirFollowLinks {
        fn is_dir_follow_links(&self) -> bool;
    }

    impl IsDirFollowLinks for PathBuf {
        fn is_dir_follow_links(&self) -> bool {
            // Although `canonicalize` is heavy, we must use it because `symlink_metadata`
            // is not reliable.
            self.canonicalize()
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
        }
    }

    let registry = &world.registry;

    // search packages locally. We only search in the data
    // directory and not the cache directory, because the latter is not
    // intended for storage of local packages.
    let mut packages = eco_vec![];

    let paths = registry.paths();
    log::info!("searching for packages in paths {paths:?}");

    let mut search_in_dir = |local_path: PathBuf, ns: EcoString| {
        if !local_path.exists() || !local_path.is_dir_follow_links() {
            return;
        }
        // namespace/package_name/version
        // 2. package_name
        let Some(package_names) = once_log(std::fs::read_dir(local_path), "read local package")
        else {
            return;
        };
        for package in package_names {
            let Some(package) = once_log(package, "read package name") else {
                continue;
            };
            let package_name = EcoString::from(package.file_name().to_string_lossy());
            if package_name.starts_with('.') {
                continue;
            }

            let package_path = package.path();
            if !package_path.is_dir_follow_links() {
                continue;
            }
            // 3. version
            let Some(versions) = once_log(std::fs::read_dir(package_path), "read package versions")
            else {
                continue;
            };
            for version in versions {
                let Some(version_entry) = once_log(version, "read package version") else {
                    continue;
                };
                if version_entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let package_version_path = version_entry.path();
                if !package_version_path.is_dir_follow_links() {
                    continue;
                }
                let Some(version) = once_log(
                    version_entry.file_name().to_string_lossy().parse(),
                    "parse package version",
                ) else {
                    continue;
                };
                let spec = PackageSpec {
                    namespace: ns.clone(),
                    name: package_name.clone(),
                    version,
                };
                let manifest_id = typst::syntax::FileId::new(typst::syntax::RootedPath::new(
                    typst::syntax::VirtualRoot::Package(spec.clone()),
                    typst::syntax::VirtualPath::new("typst.toml").expect("valid manifest path"),
                ));
                let Some(manifest) =
                    once_log(get_manifest(world, manifest_id), "read package manifest")
                else {
                    continue;
                };
                packages.push(PackageIndexEntry {
                    namespace: ns.clone(),
                    package: manifest.package,
                    template: manifest.template,
                    updated_at: None,
                    path: Some(package_version_path),
                });
            }
        }
    };

    for dir in paths {
        let matching_ns = match &filter {
            PackageFilter::For(ns) => {
                let local_path = dir.join(ns.as_str());
                search_in_dir(local_path, ns.clone());

                continue;
            }
            PackageFilter::ExceptFor(ns) => Some(ns),
            PackageFilter::All => None,
        };

        let Some(namespaces) = once_log(std::fs::read_dir(dir), "read package directory") else {
            continue;
        };
        for dir in namespaces {
            let Some(dir) = once_log(dir, "read ns directory") else {
                continue;
            };
            let ns = dir.file_name();
            let ns = ns.to_string_lossy();
            if let Some(matching_ns) = &matching_ns
                && matching_ns.as_str() == ns.as_ref()
            {
                continue;
            }
            let local_path = dir.path();
            search_in_dir(local_path, ns.into());
        }
    }

    packages
}

#[cfg(feature = "local-registry")]
fn once_log<T, E: std::fmt::Display>(result: Result<T, E>, site: &'static str) -> Option<T> {
    use std::collections::HashSet;
    use std::sync::OnceLock;

    use parking_lot::Mutex;

    let err = match result {
        Ok(value) => return Some(value),
        Err(err) => err,
    };

    static ONCE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut once = ONCE.get_or_init(Default::default).lock();
    if once.insert(site) {
        log::error!("failed to perform {site}: {err}");
    }

    None
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use typst::syntax::package::PackageSpec;

    use super::*;
    use crate::syntax::Decl;
    use crate::tests::{run_with_ctx, run_with_sources};
    use crate::ty::{SigTy, TypeVar};

    fn manifest_id() -> FileId {
        FileId::new(RootedPath::new(
            VirtualRoot::Package(
                PackageSpec::from_str("@preview/example:0.1.0").expect("valid package spec"),
            ),
            VirtualPath::new("typst.toml").expect("valid manifest path"),
        ))
    }

    #[test]
    fn package_entrypoint_id_resolves_relative_to_manifest_parent() {
        let manifest_id = manifest_id();
        let entrypoint = package_entrypoint_id(manifest_id, "src/lib.typ");

        assert_eq!(entrypoint.root(), manifest_id.root());
        assert_eq!(entrypoint.vpath().get_with_slash(), "/src/lib.typ");
    }

    #[test]
    fn package_entrypoint_id_resolves_absolute_path_in_package_root() {
        let manifest_id = manifest_id();
        let entrypoint = package_entrypoint_id(manifest_id, "/lib.typ");

        assert_eq!(entrypoint.root(), manifest_id.root());
        assert_eq!(entrypoint.vpath().get_with_slash(), "/lib.typ");
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn signature_binder_detection_visits_shared_type_once() {
        const DEPTH: usize = 16;

        let mut shared = Ty::Any;
        for _ in 0..DEPTH {
            shared = Ty::Tuple(vec![shared.clone(), shared].into());
        }
        let mut traversed = FxHashSet::default();
        assert!(!contains_signature_binders_inner(
            &shared,
            &mut traversed,
            &mut FxHashSet::default(),
        ));
        assert_eq!(traversed.len(), DEPTH + 1);

        let binder = TypeVar::new("input".into(), Decl::lit("input").into());
        let binder_ty = Ty::Var(binder);
        assert!(contains_signature_binders(&Ty::Func(SigTy::unary(
            binder_ty,
            Ty::Any,
        ))));
    }

    #[test]
    fn type_dumper_reuses_dumped_types() {
        let info = TypeInfo::default();
        let mut dumper = TypeDumper::new(
            &info,
            PackageTyckDumpOptions {
                max_type_chars: Some(128),
            },
        );

        let first = dumper.dump(Ty::Any);
        let second = dumper.dump(Ty::Any);

        assert_eq!(dumper.cache.len(), 1);
        assert_eq!(first.debug, second.debug);
        assert_eq!(first.describe, second.describe);
        assert_eq!(first.repr, second.repr);
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn principal_dump_preserves_type_guard_branches() {
        run_with_sources(
            r#"
#let auto-cast(pat) = {
  if type(pat) == dictionary {
    pat
  } else if type(pat) == array {
    pat.map(auto-cast)
  }
}
"#,
            |verse, path| {
                run_with_ctx(verse, path, &|ctx, path| {
                    let source = ctx.source_by_path(&path).unwrap();
                    let info = ctx.type_check(&source);
                    let mapped = info
                        .mapping
                        .iter()
                        .find_map(|(span, mapped)| {
                            let range = source_range(&source, *span)?;
                            (source.text().get(range) == Some("pat.map")).then_some(mapped)
                        })
                        .expect("pat.map must have a mapped type");
                    let source_ty = Ty::from_types(mapped.clone().into_iter());
                    let mut dumper = TypeDumper::new(
                        &info,
                        PackageTyckDumpOptions {
                            max_type_chars: Some(1024),
                        },
                    );
                    let dumped = dumper.dump(source_ty);

                    assert!(dumped.debug.contains("Type(array)"));
                    assert!(dumped.debug.contains("Type(dictionary)"));
                    assert!(dumped.debug.contains(".map"));
                });
            },
        );
    }

    #[test]
    fn package_tyck_files_skip_unreadable_imports() {
        run_with_sources(
            r#"
// path: present.typ
#let present = true
-----
// path: main.typ
#import "present.typ"
#import "missing.typ"
"#,
            |verse, path| {
                run_with_ctx(verse, path, &|ctx, path| {
                    let entrypoint = ctx.source_by_path(&path).unwrap().id();
                    let files = collect_package_tyck_files(ctx, entrypoint, Default::default())
                        .expect("missing imports should not abort a package scan");

                    assert_eq!(
                        files
                            .iter()
                            .map(|file| file.path.as_str())
                            .collect::<Vec<_>>(),
                        ["main.typ", "present.typ"]
                    );
                    let main = files
                        .iter()
                        .find(|file| file.path == "main.typ")
                        .expect("entrypoint must be dumped");
                    assert_eq!(
                        main.imports
                            .iter()
                            .map(|import| import.path.as_str())
                            .collect::<Vec<_>>(),
                        ["missing.typ", "present.typ"]
                    );
                });
            },
        );
    }
}
