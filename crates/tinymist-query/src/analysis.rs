//! Semantic static and dynamic analysis of the source code.

mod bib;
pub(crate) use bib::*;
pub mod call;
pub use call::*;
pub mod completion;
pub use completion::*;
pub mod code_action;
pub use code_action::*;
pub mod color_expr;
pub use color_expr::*;
pub mod doc_highlight;
pub use doc_highlight::*;
pub mod link_expr;
pub use link_expr::*;
pub mod definition;
pub use definition::*;
pub mod signature;
pub use signature::*;
pub mod semantic_tokens;
pub use semantic_tokens::*;

mod global;
mod post_tyck;
mod prelude;
mod tyck;

pub(crate) use crate::ty::*;
pub use global::*;
pub(crate) use post_tyck::*;
pub(crate) use tinymist_analysis::stats::{AnalysisStats, QueryStatGuard};
pub(crate) use tyck::*;
pub use typst_shim::syntax::VirtualPathExt;

use std::sync::Arc;

use ecow::eco_format;
use lsp_types::Url;
use tinymist_project::LspComputeGraph;
use tinymist_std::error::WithContextUntyped;
use tinymist_std::{Result, bail};
use tinymist_world::{EntryReader, EntryState, TaskInputs};
use typst::diag::{FileError, FileResult, StrResult};
use typst::foundations::{Func, Value};
use typst::syntax::FileId;

use crate::{CompilerQueryResponse, SemanticRequest, path_res_to_url};

pub(crate) trait ToFunc {
    fn to_func(&self) -> Option<Func>;
}

impl ToFunc for Value {
    fn to_func(&self) -> Option<Func> {
        match self {
            Value::Func(func) => Some(func.clone()),
            Value::Type(ty) => ty.constructor().ok(),
            _ => None,
        }
    }
}

/// Extension trait for `typst::World`.
pub trait LspWorldExt {
    /// Resolve the uri for a file id.
    fn uri_for_id(&self, fid: FileId) -> FileResult<Url>;
}

impl LspWorldExt for tinymist_project::LspWorld {
    fn uri_for_id(&self, fid: FileId) -> Result<Url, FileError> {
        let res = path_res_to_url(self.path_for_id(fid)?);

        crate::log_debug_ct!("uri_for_id: {fid:?} -> {res:?}");
        res.map_err(|err| FileError::Other(Some(eco_format!("convert to url: {err:?}"))))
    }
}

/// A snapshot for LSP queries.
pub struct LspQuerySnapshot {
    /// The using snapshot.
    pub snap: LspComputeGraph,
    /// The global shared analysis data.
    analysis: Arc<Analysis>,
    /// The revision lock for the analysis (cache).
    rev_lock: AnalysisRevLock,
}

impl std::ops::Deref for LspQuerySnapshot {
    type Target = LspComputeGraph;

    fn deref(&self) -> &Self::Target {
        &self.snap
    }
}

impl LspQuerySnapshot {
    /// Runs a query for another task.
    pub fn task(mut self, inputs: TaskInputs) -> Self {
        self.snap = self.snap.task(inputs);
        self
    }

    /// Runs a semantic query.
    pub fn run_semantic<T: SemanticRequest>(
        self,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> Result<CompilerQueryResponse> {
        self.run_analysis(|ctx| query.request(ctx)).map(wrapper)
    }

    /// Runs a query.
    pub fn run_analysis<T>(self, f: impl FnOnce(&mut LocalContextGuard) -> T) -> Result<T> {
        let graph = self.snap.clone();
        let Some(..) = graph.world().main_id() else {
            log::error!("Project: main file is not set");
            bail!("main file is not set");
        };

        let mut ctx = self.analysis.enter_(graph, self.rev_lock);
        Ok(f(&mut ctx))
    }

    /// Checks within package
    pub fn run_within_package<T>(
        self,
        info: &crate::package::PackageInfo,
        f: impl FnOnce(&mut LocalContextGuard) -> Result<T> + Send + Sync,
    ) -> Result<T> {
        let world = self.world();

        let entry: StrResult<EntryState> = Ok(()).and_then(|_| {
            let toml_id = crate::package::get_manifest_id(info)?;
            let toml_path = world.path_for_id(toml_id)?.as_path().to_owned();
            let pkg_root = toml_path
                .parent()
                .ok_or_else(|| eco_format!("cannot get package root (parent of {toml_path:?})"))?;

            let manifest = crate::package::get_manifest(world, toml_id)?;
            let entry_point =
                crate::package::package_entrypoint_id(toml_id, &manifest.package.entrypoint);

            Ok(EntryState::new_rooted_by_id(pkg_root.into(), entry_point))
        });
        let entry = entry.context_ut("resolve package entry")?;

        let snap = self.task(TaskInputs {
            entry: Some(entry),
            inputs: None,
        });

        snap.run_analysis(f)?
    }
}

#[cfg(test)]
mod matcher_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::{syntax::classify_def, tests::*};

    #[test]
    fn test() {
        snapshot_testing("match_def", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos).unwrap();

            let snap = classify_def(node).map(|def| format!("{:?}", def.node().range()));
            let snap = snap.as_deref().unwrap_or("<nil>");

            assert_snapshot!(snap);
        });
    }
}

#[cfg(test)]
mod expr_tests {

    use tinymist_std::path::unix_slash;
    use tinymist_world::vfs::WorkspaceResolver;
    use typst::syntax::Source;
    use typst_shim::syntax::{RootedPathExt, VirtualPathExt, source_range};

    use crate::syntax::{Expr, RefExpr};
    use crate::tests::*;

    trait ShowExpr {
        fn show_expr(&self, expr: &Expr) -> String;
    }

    impl ShowExpr for Source {
        fn show_expr(&self, node: &Expr) -> String {
            match node {
                Expr::Decl(decl) => {
                    let range = source_range(self, decl.span()).unwrap_or_default();
                    let fid = if let Some(fid) = decl.file_id() {
                        if WorkspaceResolver::is_package_file(fid) {
                            let package = fid.package_compat().expect("package file");
                            format!(
                                " in {package:?}{}",
                                unix_slash(fid.vpath().as_rooted_path_compat())
                            )
                        } else {
                            format!(" in {}", unix_slash(fid.vpath().as_rooted_path_compat()))
                        }
                    } else {
                        "".to_string()
                    };
                    format!("{decl:?}@{range:?}{fid}")
                }
                _ => format!("{node}"),
            }
        }
    }

    #[test]
    fn docs() {
        snapshot_testing("docs", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut docstrings = result.docstrings.iter().collect::<Vec<_>>();
            docstrings.sort_by(|x, y| x.0.cmp(y.0));
            let mut docstrings = docstrings
                .into_iter()
                .map(|(ident, expr)| {
                    format!(
                        "{} -> {expr:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                    )
                })
                .collect::<Vec<_>>();
            let mut snap = vec![];
            snap.push("= docstings".to_owned());
            snap.append(&mut docstrings);

            assert_snapshot!(snap.join("\n"));
        });
    }

    #[test]
    fn scope() {
        snapshot_testing("expr_of", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut resolves = result.resolves.iter().collect::<Vec<_>>();
            resolves.sort_by(|x, y| x.1.decl.cmp(&y.1.decl));

            let mut resolves = resolves
                .into_iter()
                .map(|(_, expr)| {
                    let RefExpr {
                        decl: ident,
                        step,
                        root,
                        term,
                    } = expr.as_ref();

                    format!(
                        "{} -> {}, root {}, val: {term:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                        step.as_ref()
                            .map(|expr| source.show_expr(expr))
                            .unwrap_or_default(),
                        root.as_ref()
                            .map(|expr| source.show_expr(expr))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>();
            let mut exports = result.exports.iter().collect::<Vec<_>>();
            exports.sort_by(|x, y| x.0.cmp(y.0));
            let mut exports = exports
                .into_iter()
                .map(|(ident, node)| {
                    let node = source.show_expr(node);
                    format!("{ident} -> {node}",)
                })
                .collect::<Vec<_>>();

            let mut snap = vec![];
            snap.push("= resolves".to_owned());
            snap.append(&mut resolves);
            snap.push("= exports".to_owned());
            snap.append(&mut exports);

            assert_snapshot!(snap.join("\n"));
        });
    }
}

#[cfg(test)]
mod module_tests {
    use serde_json::json;
    use tinymist_std::path::unix_slash;
    use typst::syntax::FileId;

    use crate::prelude::*;
    use crate::syntax::module::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("modules", &|ctx, _| {
            fn ids(ids: EcoVec<FileId>) -> Vec<String> {
                let mut ids: Vec<String> = ids
                    .into_iter()
                    .map(|id| unix_slash(id.vpath().as_rooted_path_compat()))
                    .collect();
                ids.sort();
                ids
            }

            let dependencies = construct_module_dependencies(ctx);

            let mut dependencies = dependencies
                .into_iter()
                .map(|(id, v)| {
                    (
                        unix_slash(id.vpath().as_rooted_path_compat()),
                        ids(v.dependencies),
                        ids(v.dependents),
                    )
                })
                .collect::<Vec<_>>();

            dependencies.sort();
            // remove /main.typ
            dependencies.retain(|(path, _, _)| path != "/main.typ");

            let dependencies = dependencies
                .into_iter()
                .map(|(id, deps, dependents)| {
                    let mut mp = serde_json::Map::new();
                    mp.insert("id".to_string(), json!(id));
                    mp.insert("dependencies".to_string(), json!(deps));
                    mp.insert("dependents".to_string(), json!(dependents));
                    json!(mp)
                })
                .collect::<Vec<_>>();

            assert_snapshot!(JsonRepr::new_pure(dependencies));
        });
    }
}

#[cfg(test)]
mod type_check_tests {

    use core::fmt;

    use typst::syntax::Source;

    use crate::syntax::Decl;
    use crate::tests::*;
    use typst_shim::syntax::source_range;

    use super::{Ty, TypeInfo};

    #[test]
    fn test() {
        snapshot_testing("type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.type_check(&source);
            let result = format!("{:#?}", TypeCheckSnapshot(&source, &result));

            assert_snapshot!(result);
        });
    }

    struct TypeCheckSnapshot<'a>(&'a Source, &'a TypeInfo);

    impl fmt::Debug for TypeCheckSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let source = self.0;
            let info = self.1;
            let mut vars = info
                .vars
                .values()
                .filter(|bounds| !matches!(bounds.var.def.as_ref(), Decl::Generated(_)))
                .map(|bounds| (bounds.name(), bounds))
                .collect::<Vec<_>>();

            vars.sort_by(|x, y| x.1.var.strict_cmp(&y.1.var));

            for (name, bounds) in vars {
                writeln!(f, "{name:?} = {:?}", info.simplify(bounds.as_type(), true))?;
            }

            writeln!(f, "=====")?;
            let mut mapping = info
                .mapping
                .iter()
                .map(|pair| (source_range(source, *pair.0).unwrap_or_default(), pair.1))
                .collect::<Vec<_>>();

            mapping.sort_by(|x, y| {
                x.0.start
                    .cmp(&y.0.start)
                    .then_with(|| x.0.end.cmp(&y.0.end))
            });

            for (range, value) in mapping {
                let ty = Ty::from_types(value.clone().into_iter());
                let ty = if has_generated_var(&ty) {
                    info.simplify(ty, true)
                } else {
                    ty
                };
                writeln!(f, "{range:?} -> {ty:?}")?;
            }

            Ok(())
        }
    }

    fn has_generated_var(ty: &Ty) -> bool {
        match ty {
            Ty::Var(var) => matches!(var.def.as_ref(), Decl::Generated(_)),
            Ty::Param(param) => has_generated_var(&param.ty),
            Ty::Array(elem) => has_generated_var(elem),
            Ty::Tuple(elems) | Ty::Union(elems) => elems.iter().any(has_generated_var),
            Ty::Dict(record) => record.types.iter().any(has_generated_var),
            Ty::Func(_) | Ty::Args(_) | Ty::Pattern(_) | Ty::With(_) => false,
            Ty::Apply(apply) => {
                has_generated_var(&apply.callee)
                    || apply.args.inputs().any(has_generated_var)
                    || apply.args.body.as_ref().is_some_and(has_generated_var)
            }
            Ty::Select(select) => has_generated_var(&select.ty),
            Ty::Unary(unary) => has_generated_var(&unary.lhs),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                has_generated_var(lhs) || has_generated_var(rhs)
            }
            Ty::If(if_ty) => {
                has_generated_var(&if_ty.cond)
                    || has_generated_var(&if_ty.then)
                    || has_generated_var(&if_ty.else_)
            }
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(has_generated_var),
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
        }
    }
}

#[cfg(test)]
mod package_type_scan_tests {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use rayon::prelude::*;
    use serde::{Deserialize, Serialize};
    use tinymist_project::{
        CompileFontArgs, DynAccessModel, EntryManager, EntryState, ExportTarget, LspUniverseBuilder,
    };
    use tinymist_world::args::CompilePackageArgs;
    use tinymist_world::vfs::system::SystemAccessModel;
    use typst::syntax::{Source, VirtualPath};
    use typst_shim::syntax::source_range;
    use walkdir::WalkDir;

    use crate::analysis::Analysis;
    use crate::syntax::{DeclExpr, Expr, Pattern};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct LetTyping {
        file: String,
        range: String,
        name: String,
        kind: String,
        ty: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct FileScan {
        file: String,
        elapsed_ms: f64,
        let_count: usize,
        status: String,
        error: Option<String>,
    }

    #[derive(Debug, Default, Serialize, Deserialize)]
    struct PackageScan {
        package: String,
        files: Vec<FileScan>,
        typings: Vec<LetTyping>,
    }

    #[derive(Debug, Serialize)]
    struct Summary {
        root: String,
        output: String,
        package_count: usize,
        file_count: usize,
        ok_count: usize,
        error_count: usize,
        typing_count: usize,
        elapsed_ms_total: f64,
        elapsed_ms_p50: f64,
        elapsed_ms_p90: f64,
        elapsed_ms_p99: f64,
        slowest_files: Vec<FileScan>,
    }

    #[test]
    #[ignore = "set TINYMIST_SCAN_TYPST_PACKAGES=1 to scan ~/work/typst/packages/packages"]
    fn scan_typst_packages_type_check() {
        if std::env::var_os("TINYMIST_SCAN_TYPST_PACKAGES").is_none() {
            return;
        }

        let package = std::env::var("TINYMIST_SCAN_TYPST_PACKAGES_CHILD_PACKAGE").ok();
        std::thread::Builder::new()
            .name("scan_typst_packages_type_check".to_owned())
            .stack_size(128 * 1024 * 1024)
            .spawn(move || match package {
                Some(package) => run_scan(Some(&package)),
                None => run_parent_scan(),
            })
            .expect("failed to spawn scanner thread")
            .join()
            .expect("scanner thread panicked");
    }

    fn run_parent_scan() {
        let package_root = package_root();
        let out_root = scan_out_root();
        let packages_out = out_root.join("packages");
        std::fs::create_dir_all(&packages_out).expect("failed to create output directory");

        let mut packages = typst_files(&package_root)
            .into_iter()
            .map(|path| package_key(&package_root, &path))
            .collect::<Vec<_>>();
        packages.sort();
        packages.dedup();

        let exe = std::env::current_exe().expect("failed to get current test executable");
        let test_name = "analysis::package_type_scan_tests::scan_typst_packages_type_check";
        let jobs = std::env::var("TINYMIST_SCAN_TYPST_PACKAGES_JOBS")
            .ok()
            .and_then(|jobs| jobs.parse::<usize>().ok())
            .filter(|jobs| *jobs > 0)
            .unwrap_or(8);
        let child_timeout = std::env::var("TINYMIST_SCAN_TYPST_PACKAGES_CHILD_TIMEOUT_SECS")
            .ok()
            .and_then(|secs| secs.parse::<u64>().ok())
            .filter(|secs| *secs > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(120));
        std::fs::write(
            out_root.join("stage.txt"),
            format!(
                "parallel-packages:{}:jobs:{jobs}:child-timeout:{}s",
                packages.len(),
                child_timeout.as_secs()
            ),
        )
        .expect("failed to write scanner stage");

        let started = Instant::now();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .expect("failed to build package scanner pool");
        let package_reports = pool.install(|| {
            packages
                .par_iter()
                .map(|package| {
                    let package_started = Instant::now();
                    let mut child = std::process::Command::new(&exe)
                        .arg(test_name)
                        .arg("--ignored")
                        .arg("--exact")
                        .env("TINYMIST_SCAN_TYPST_PACKAGES", "1")
                        .env("TINYMIST_SCAN_TYPST_PACKAGES_CHILD_PACKAGE", package)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .expect("failed to launch package scanner child");
                    let status = loop {
                        match child.try_wait() {
                            Ok(Some(status)) => break Ok(status),
                            Ok(None) if package_started.elapsed() >= child_timeout => {
                                let _ = child.kill();
                                let _ = child.wait();
                                break Err(format!(
                                    "child timed out after {:.2} ms",
                                    package_started.elapsed().as_secs_f64() * 1000.0
                                ));
                            }
                            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                            Err(err) => break Err(format!("failed to wait for child: {err}")),
                        }
                    };

                    let base = sanitize_package_name(package);
                    let package_json = packages_out.join(format!("{base}.json"));
                    let report = match status {
                        Ok(status) if status.success() => std::fs::read_to_string(&package_json)
                            .ok()
                            .and_then(|data| serde_json::from_str::<PackageScan>(&data).ok())
                            .unwrap_or_else(|| {
                                let elapsed_ms = package_started.elapsed().as_secs_f64() * 1000.0;
                                PackageScan {
                                    package: package.clone(),
                                    files: vec![FileScan {
                                        file: package.clone(),
                                        elapsed_ms,
                                        let_count: 0,
                                        status: "missing-child-output".to_owned(),
                                        error: Some(
                                            "child succeeded but package json was not readable"
                                                .to_owned(),
                                        ),
                                    }],
                                    typings: vec![],
                                }
                            }),
                        Ok(status) => {
                            let elapsed_ms = package_started.elapsed().as_secs_f64() * 1000.0;
                            PackageScan {
                                package: package.clone(),
                                files: vec![FileScan {
                                    file: package.clone(),
                                    elapsed_ms,
                                    let_count: 0,
                                    status: "child-abort".to_owned(),
                                    error: Some(format!("child exited with {status}")),
                                }],
                                typings: vec![],
                            }
                        }
                        Err(error) => {
                            let elapsed_ms = package_started.elapsed().as_secs_f64() * 1000.0;
                            PackageScan {
                                package: package.clone(),
                                files: vec![FileScan {
                                    file: package.clone(),
                                    elapsed_ms,
                                    let_count: 0,
                                    status: "child-timeout".to_owned(),
                                    error: Some(error),
                                }],
                                typings: vec![],
                            }
                        }
                    };

                    std::fs::write(
                        packages_out.join(format!("{base}.md")),
                        render_package_md(&report),
                    )
                    .expect("failed to write package markdown");
                    std::fs::write(
                        package_json,
                        serde_json::to_string_pretty(&report)
                            .expect("failed to encode package json"),
                    )
                    .expect("failed to write package json");
                    (package.clone(), report)
                })
                .collect::<BTreeMap<_, _>>()
        });

        let summary = build_summary(
            &package_root,
            &out_root,
            &package_reports,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        std::fs::write(
            out_root.join("summary.md"),
            render_summary_md(&summary, &package_reports),
        )
        .expect("failed to write summary markdown");
        std::fs::write(
            out_root.join("packages/index.md"),
            render_package_index_md(&package_reports),
        )
        .expect("failed to write package index markdown");
        std::fs::write(
            out_root.join("errors.md"),
            render_errors_md(&package_reports),
        )
        .expect("failed to write errors markdown");
        std::fs::write(
            out_root.join("summary.json"),
            serde_json::to_string_pretty(&summary).expect("failed to encode summary json"),
        )
        .expect("failed to write summary json");
    }

    fn run_scan(package_filter: Option<&str>) {
        let package_root = package_root();
        let out_root = scan_out_root();
        let packages_out = out_root.join("packages");
        std::fs::create_dir_all(&packages_out).expect("failed to create output directory");

        let mut files = typst_files(&package_root);
        if let Some(package) = package_filter {
            files.retain(|path| package_key(&package_root, path) == package);
        }
        let Some(first) = files.first().cloned() else {
            return;
        };
        let package_name = package_filter
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| package_key(&package_root, &first));
        let entry = package_entry_path(&package_root, package_filter, &files)
            .unwrap_or_else(|| first.clone());

        std::fs::write(
            out_root.join("stage.txt"),
            format!("build-universe:{package_name}"),
        )
        .expect("failed to write scanner stage");

        let mut verse = LspUniverseBuilder::build(
            EntryState::new_rooted(package_root.as_path().into(), None),
            ExportTarget::Paged,
            Default::default(),
            Default::default(),
            LspUniverseBuilder::resolve_package(
                None,
                Some(&CompilePackageArgs {
                    package_path: Some(package_root.clone()),
                    package_cache_path: Some(package_root.clone()),
                }),
            ),
            Arc::new(
                LspUniverseBuilder::resolve_fonts(CompileFontArgs {
                    ignore_system_fonts: true,
                    ..Default::default()
                })
                .expect("failed to resolve fonts"),
            ),
            None,
            DynAccessModel(Arc::new(SystemAccessModel {})),
        );

        verse
            .mutate_entry(EntryState::new_rooted(
                package_root.as_path().into(),
                Some(
                    VirtualPath::virtualize(&package_root, &entry)
                        .expect("scan entry file must be under root"),
                ),
            ))
            .expect("failed to set entry");

        let analysis = Arc::new(Analysis::default());
        let mut ctx = analysis.enter(verse.computation());
        let mut pkg = PackageScan {
            package: package_name,
            ..Default::default()
        };
        for path in files {
            scan_file(&package_root, &out_root, &mut ctx, &path, &mut pkg);
        }

        let base = sanitize_package_name(&pkg.package);
        std::fs::write(
            packages_out.join(format!("{base}.md")),
            render_package_md(&pkg),
        )
        .expect("failed to write package markdown");
        std::fs::write(
            packages_out.join(format!("{base}.json")),
            serde_json::to_string_pretty(&pkg).expect("failed to encode package json"),
        )
        .expect("failed to write package json");
    }

    fn package_root() -> PathBuf {
        let home = std::env::var_os("HOME").expect("HOME must be set");
        std::env::var_os("TINYMIST_SCAN_TYPST_PACKAGES_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join("work/typst/packages/packages"))
            .canonicalize()
            .expect("package root must exist")
    }

    fn package_entry_path(
        package_root: &Path,
        package_filter: Option<&str>,
        files: &[PathBuf],
    ) -> Option<PathBuf> {
        let package = package_filter?;
        let manifest = package_root.join(package).join("typst.toml");
        let manifest = std::fs::read_to_string(manifest).ok()?;
        let manifest = manifest.parse::<toml::Value>().ok()?;
        let entrypoint = manifest
            .get("package")
            .and_then(|package| package.get("entrypoint"))
            .and_then(|entrypoint| entrypoint.as_str())?;
        let entry = package_root.join(package).join(entrypoint);
        files.iter().any(|path| path == &entry).then_some(entry)
    }

    fn scan_out_root() -> PathBuf {
        let label = std::env::var("TINYMIST_SCAN_TYPST_PACKAGES_LABEL")
            .unwrap_or_else(|_| "current".to_owned());
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crate must live under workspace/crates")
            .join("target/tyck-package-scan")
            .join(label)
    }

    fn typst_files(package_root: &Path) -> Vec<PathBuf> {
        let mut files = WalkDir::new(package_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "typ"))
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn scan_file(
        package_root: &Path,
        out_root: &Path,
        ctx: &mut crate::analysis::LocalContextGuard,
        path: &Path,
        pkg: &mut PackageScan,
    ) {
        let rel = rel_path(package_root, path);
        std::fs::write(out_root.join("current.txt"), &rel)
            .expect("failed to write scanner progress");
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let source = ctx.source_by_path(path)?;
            let expr_info = ctx.expr_stage(&source);
            std::fs::write(out_root.join("current.txt"), format!("{rel}: type_check"))
                .expect("failed to write scanner progress");
            let type_info = ctx.type_check(&source);
            let mut decls = vec![];
            collect_let_decls(&expr_info.root, &mut decls);
            let typings = decls
                .into_iter()
                .map(|decl| {
                    std::fs::write(
                        out_root.join("current.txt"),
                        format!(
                            "{}: simplify {} {}",
                            rel,
                            decl.name(),
                            format_range(&source, decl.span())
                        ),
                    )
                    .expect("failed to write scanner progress");
                    let ty = type_info
                        .vars
                        .get(&decl)
                        .map(|bounds| format!("{:?}", type_info.simplify(bounds.as_type(), true)))
                        .unwrap_or_else(|| "<missing>".to_owned());
                    LetTyping {
                        file: rel.clone(),
                        range: format_range(&source, decl.span()),
                        name: decl.name().to_string(),
                        kind: decl.kind().to_string(),
                        ty,
                    }
                })
                .collect::<Vec<_>>();
            Ok::<_, typst::diag::FileError>(typings)
        }));

        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        match outcome {
            Ok(Ok(typings)) => {
                pkg.files.push(FileScan {
                    file: rel,
                    elapsed_ms,
                    let_count: typings.len(),
                    status: "ok".to_owned(),
                    error: None,
                });
                pkg.typings.extend(typings);
            }
            Ok(Err(err)) => pkg.files.push(FileScan {
                file: rel,
                elapsed_ms,
                let_count: 0,
                status: "error".to_owned(),
                error: Some(format!("{err:?}")),
            }),
            Err(err) => pkg.files.push(FileScan {
                file: rel,
                elapsed_ms,
                let_count: 0,
                status: "panic".to_owned(),
                error: Some(panic_payload(err)),
            }),
        }
    }

    fn collect_let_decls(expr: &Expr, out: &mut Vec<DeclExpr>) {
        match expr {
            Expr::Block(exprs) => {
                for expr in exprs.iter() {
                    collect_let_decls(expr, out);
                }
            }
            Expr::Func(func) => {
                out.push(func.decl.clone());
                collect_let_decls(&func.body, out);
            }
            Expr::Let(let_expr) => {
                collect_pattern_decls(&let_expr.pattern, out);
                if let Some(body) = &let_expr.body {
                    collect_let_decls(body, out);
                }
            }
            Expr::Array(args) | Expr::Dict(args) | Expr::Args(args) => {
                for arg in args.args.iter() {
                    match arg {
                        crate::syntax::ArgExpr::Pos(expr)
                        | crate::syntax::ArgExpr::Spread(expr) => collect_let_decls(expr, out),
                        crate::syntax::ArgExpr::Named(pair) => collect_let_decls(&pair.1, out),
                        crate::syntax::ArgExpr::NamedRt(pair) => collect_let_decls(&pair.1, out),
                    }
                }
            }
            Expr::Pattern(pattern) => collect_pattern_decls(pattern, out),
            Expr::Element(element) => {
                for child in element.content.iter() {
                    collect_let_decls(child, out);
                }
            }
            Expr::Unary(unary) => collect_let_decls(&unary.lhs, out),
            Expr::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                collect_let_decls(lhs, out);
                collect_let_decls(rhs, out);
            }
            Expr::Apply(apply) => {
                collect_let_decls(&apply.callee, out);
                collect_let_decls(&apply.args, out);
            }
            Expr::Show(show) => {
                if let Some(selector) = &show.selector {
                    collect_let_decls(selector, out);
                }
                collect_let_decls(&show.edit, out);
            }
            Expr::Set(set) => {
                collect_let_decls(&set.target, out);
                collect_let_decls(&set.args, out);
                if let Some(cond) = &set.cond {
                    collect_let_decls(cond, out);
                }
            }
            Expr::ContentRef(content_ref) => {
                if let Some(body) = &content_ref.body {
                    collect_let_decls(body, out);
                }
            }
            Expr::Select(select) => collect_let_decls(&select.lhs, out),
            Expr::Import(import) => collect_let_decls(&import.source, out),
            Expr::Include(include) => collect_let_decls(&include.source, out),
            Expr::Contextual(expr) => collect_let_decls(expr, out),
            Expr::Conditional(if_expr) => {
                collect_let_decls(&if_expr.cond, out);
                collect_let_decls(&if_expr.then, out);
                collect_let_decls(&if_expr.else_, out);
            }
            Expr::WhileLoop(while_expr) => {
                collect_let_decls(&while_expr.cond, out);
                collect_let_decls(&while_expr.body, out);
            }
            Expr::ForLoop(for_expr) => {
                collect_pattern_decls(&for_expr.pattern, out);
                collect_let_decls(&for_expr.iter, out);
                collect_let_decls(&for_expr.body, out);
            }
            Expr::Ref(_) | Expr::Decl(_) | Expr::Type(_) | Expr::Star => {}
        }
    }

    fn collect_pattern_decls(pattern: &Pattern, out: &mut Vec<DeclExpr>) {
        match pattern {
            Pattern::Expr(expr) => collect_let_decls(expr, out),
            Pattern::Simple(decl) => out.push(decl.clone()),
            Pattern::Sig(sig) => {
                for pattern in sig.pos.iter() {
                    collect_pattern_decls(pattern, out);
                }
                for (_, pattern) in sig.named.iter() {
                    collect_pattern_decls(pattern, out);
                }
            }
        }
    }

    fn build_summary(
        root: &Path,
        out_root: &Path,
        packages: &BTreeMap<String, PackageScan>,
        elapsed_ms_total: f64,
    ) -> Summary {
        let mut files = packages
            .values()
            .flat_map(|pkg| pkg.files.iter().cloned())
            .collect::<Vec<_>>();
        let mut timings = files.iter().map(|file| file.elapsed_ms).collect::<Vec<_>>();
        timings.sort_by(f64::total_cmp);
        files.sort_by(|a, b| b.elapsed_ms.total_cmp(&a.elapsed_ms));

        Summary {
            root: root.display().to_string(),
            output: out_root.display().to_string(),
            package_count: packages.len(),
            file_count: timings.len(),
            ok_count: packages
                .values()
                .flat_map(|pkg| pkg.files.iter())
                .filter(|file| file.status == "ok")
                .count(),
            error_count: packages
                .values()
                .flat_map(|pkg| pkg.files.iter())
                .filter(|file| file.status != "ok")
                .count(),
            typing_count: packages.values().map(|pkg| pkg.typings.len()).sum(),
            elapsed_ms_total,
            elapsed_ms_p50: percentile(&timings, 0.50),
            elapsed_ms_p90: percentile(&timings, 0.90),
            elapsed_ms_p99: percentile(&timings, 0.99),
            slowest_files: files.into_iter().take(50).collect(),
        }
    }

    fn percentile(sorted: &[f64], q: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
        sorted[idx]
    }

    fn render_summary_md(summary: &Summary, packages: &BTreeMap<String, PackageScan>) -> String {
        let mut out = String::new();
        writeln!(out, "# Typst Package Type Check Scan").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "- Root: `{}`", summary.root).unwrap();
        writeln!(out, "- Output: `{}`", summary.output).unwrap();
        writeln!(out, "- Packages: {}", summary.package_count).unwrap();
        writeln!(out, "- Files: {}", summary.file_count).unwrap();
        writeln!(out, "- OK: {}", summary.ok_count).unwrap();
        writeln!(out, "- Errors: {}", summary.error_count).unwrap();
        writeln!(out, "- `#let` typings: {}", summary.typing_count).unwrap();
        writeln!(out, "- Total elapsed: {:.2} ms", summary.elapsed_ms_total).unwrap();
        writeln!(out, "- Per-file p50: {:.2} ms", summary.elapsed_ms_p50).unwrap();
        writeln!(out, "- Per-file p90: {:.2} ms", summary.elapsed_ms_p90).unwrap();
        writeln!(out, "- Per-file p99: {:.2} ms", summary.elapsed_ms_p99).unwrap();
        writeln!(out).unwrap();
        writeln!(out, "## Slowest Files").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| elapsed ms | lets | status | file |").unwrap();
        writeln!(out, "|---:|---:|---|---|").unwrap();
        for file in &summary.slowest_files {
            writeln!(
                out,
                "| {:.2} | {} | {} | `{}` |",
                file.elapsed_ms, file.let_count, file.status, file.file
            )
            .unwrap();
        }
        writeln!(out).unwrap();
        writeln!(out, "## Packages").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| package | files | lets | errors | report |").unwrap();
        writeln!(out, "|---|---:|---:|---:|---|").unwrap();
        for pkg in packages.values() {
            let file_count = pkg.files.len();
            let error_count = pkg.files.iter().filter(|file| file.status != "ok").count();
            let base = sanitize_package_name(&pkg.package);
            writeln!(
                out,
                "| `{}` | {} | {} | {} | [md](packages/{base}.md) / [json](packages/{base}.json) |",
                pkg.package,
                file_count,
                pkg.typings.len(),
                error_count,
            )
            .unwrap();
        }
        out
    }

    fn render_package_index_md(packages: &BTreeMap<String, PackageScan>) -> String {
        let mut out = String::from("# Package Reports\n\n");
        for pkg in packages.values() {
            let base = sanitize_package_name(&pkg.package);
            writeln!(
                out,
                "- `{}`: [md]({base}.md), [json]({base}.json)",
                pkg.package
            )
            .unwrap();
        }
        out
    }

    fn render_errors_md(packages: &BTreeMap<String, PackageScan>) -> String {
        let mut out = String::from("# Type Check Errors\n\n");
        for pkg in packages.values() {
            let errors = pkg
                .files
                .iter()
                .filter(|file| file.status != "ok")
                .collect::<Vec<_>>();
            if errors.is_empty() {
                continue;
            }
            writeln!(out, "## `{}`\n", pkg.package).unwrap();
            for file in errors {
                writeln!(
                    out,
                    "- `{}` ({:.2} ms, {}): `{}`",
                    file.file,
                    file.elapsed_ms,
                    file.status,
                    file.error.as_deref().unwrap_or("")
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }
        out
    }

    fn render_package_md(pkg: &PackageScan) -> String {
        let mut out = String::new();
        writeln!(out, "# `{}`", pkg.package).unwrap();
        writeln!(out).unwrap();
        writeln!(out, "- Files: {}", pkg.files.len()).unwrap();
        writeln!(out, "- `#let` typings: {}", pkg.typings.len()).unwrap();
        writeln!(
            out,
            "- Errors: {}",
            pkg.files.iter().filter(|file| file.status != "ok").count()
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "## Files").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| elapsed ms | lets | status | file |").unwrap();
        writeln!(out, "|---:|---:|---|---|").unwrap();
        for file in &pkg.files {
            writeln!(
                out,
                "| {:.2} | {} | {} | `{}` |",
                file.elapsed_ms, file.let_count, file.status, file.file
            )
            .unwrap();
        }
        writeln!(out).unwrap();
        writeln!(out, "## Typings").unwrap();
        let mut current_file = "";
        for typing in &pkg.typings {
            if current_file != typing.file {
                current_file = &typing.file;
                writeln!(out).unwrap();
                writeln!(out, "### `{current_file}`").unwrap();
                writeln!(out).unwrap();
                writeln!(out, "| range | kind | name | type |").unwrap();
                writeln!(out, "|---|---|---|---|").unwrap();
            }
            writeln!(
                out,
                "| `{}` | {} | `{}` | `{}` |",
                typing.range,
                typing.kind,
                escape_md(&typing.name),
                escape_md(&typing.ty)
            )
            .unwrap();
        }
        out
    }

    fn format_range(source: &Source, span: typst::syntax::Span) -> String {
        source_range(source, span)
            .map(|range| format!("{}..{}", range.start, range.end))
            .unwrap_or_else(|| "detached".to_owned())
    }

    fn rel_path(root: &Path, path: &Path) -> String {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn package_key(root: &Path, path: &Path) -> String {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let mut components = rel.components().filter_map(|component| {
            let std::path::Component::Normal(part) = component else {
                return None;
            };
            Some(part.to_string_lossy().into_owned())
        });
        match (components.next(), components.next(), components.next()) {
            (Some(namespace), Some(name), Some(version)) => {
                format!("{namespace}/{name}/{version}")
            }
            _ => "__unknown__".to_owned(),
        }
    }

    fn sanitize_package_name(name: &str) -> String {
        name.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                    ch
                } else {
                    '-'
                }
            })
            .collect()
    }

    fn escape_md(text: &str) -> String {
        text.replace('`', "\\`").replace('\n', " ")
    }

    fn panic_payload(err: Box<dyn std::any::Any + Send>) -> String {
        if let Some(msg) = err.downcast_ref::<&str>() {
            (*msg).to_owned()
        } else if let Some(msg) = err.downcast_ref::<String>() {
            msg.clone()
        } else {
            "unknown panic".to_owned()
        }
    }
}

#[cfg(test)]
mod post_type_check_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("post_type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().full_text();

            let result = ctx.type_check(&source);
            let post_ty = post_type_check(ctx.shared_(), &result, node);

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let post_ty = post_ty.map(|ty| format!("{ty:#?}"))
                    .unwrap_or_else(|| "<nil>".to_string());
                assert_snapshot!(post_ty);
            })
        });
    }
}

#[cfg(test)]
mod type_describe_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("type_describe", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().full_text();

            let ti = ctx.type_check(&source);
            let post_ty = post_type_check(ctx.shared_(), &ti, node);

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let post_ty = post_ty.and_then(|ty| ty.describe())
                    .unwrap_or_else(|| "<nil>".into());
                assert_snapshot!(post_ty);
            })
        });
    }
}

#[cfg(test)]
mod signature_tests {

    use core::fmt;

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::{Signature, SignatureTarget, analyze_signature};
    use crate::syntax::classify_syntax;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("signature", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let callee_node = root.leaf_at_compat(pos).unwrap();
            let callee_node = classify_syntax(callee_node, pos).unwrap();
            let callee_node = callee_node.node();

            let result = analyze_signature(
                ctx.shared(),
                SignatureTarget::Syntax(source.clone(), callee_node.span()),
            );

            assert_snapshot!(SignatureSnapshot(result.as_ref()));
        });
    }

    struct SignatureSnapshot<'a>(pub Option<&'a Signature>);

    impl fmt::Display for SignatureSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(sig) = self.0 else {
                return write!(f, "<nil>");
            };

            let primary_sig = match sig {
                Signature::Primary(sig) => sig,
                Signature::Partial(sig) => {
                    for w in &sig.with_stack {
                        write!(f, "with ")?;
                        for arg in &w.items {
                            if let Some(name) = &arg.name {
                                write!(f, "{name}: ")?;
                            }
                            let term = arg.term.as_ref();
                            let term = term.and_then(|v| v.describe()).unwrap_or_default();
                            write!(f, "{term}, ")?;
                        }
                        f.write_str("\n")?;
                    }

                    &sig.signature
                }
            };

            writeln!(f, "fn(")?;
            for param in primary_sig.pos() {
                writeln!(f, " {},", param.name)?;
            }
            for param in primary_sig.named() {
                if let Some(expr) = &param.default {
                    writeln!(f, " {}: {},", param.name, expr)?;
                } else {
                    writeln!(f, " {},", param.name)?;
                }
            }
            if let Some(param) = primary_sig.rest() {
                writeln!(f, " ...{}, ", param.name)?;
            }
            write!(f, ")")?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod call_info_tests {

    use core::fmt;

    use typst::syntax::{LinkedNode, SyntaxKind};
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::analyze_call;
    use crate::tests::*;

    use super::CallInfo;

    #[test]
    fn test() {
        snapshot_testing("call_info", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let mut call_node = root.leaf_at_compat(pos + 1).unwrap();

            while let Some(parent) = call_node.parent() {
                if call_node.kind() == SyntaxKind::FuncCall {
                    break;
                }
                call_node = parent.clone();
            }

            let result = analyze_call(ctx, source.clone(), call_node);

            assert_snapshot!(CallSnapshot(result.as_deref()));
        });
    }

    struct CallSnapshot<'a>(pub Option<&'a CallInfo>);

    impl fmt::Display for CallSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(ci) = self.0 else {
                return write!(f, "<nil>");
            };

            let mut w = ci.arg_mapping.iter().collect::<Vec<_>>();
            w.sort_by(|x, y| x.0.span().into_raw().cmp(&y.0.span().into_raw()));

            for (arg, arg_call_info) in w {
                writeln!(f, "{} -> {:?}", arg.clone().full_text(), arg_call_info)?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod lint_tests {
    use std::collections::BTreeMap;

    use tinymist_lint::KnownIssues;

    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("lint", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.lint(&source, &KnownIssues::default());
            let result = crate::diagnostics::DiagWorker::new(ctx).convert_all(result.iter());
            let result = result
                .into_iter()
                .map(|(k, v)| (file_uri_(&k), v))
                .collect::<BTreeMap<_, _>>();
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
