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
pub mod stats;
pub use stats::*;
pub mod definition;
pub use definition::*;
pub mod signature;
pub use signature::*;
pub mod semantic_tokens;
pub use semantic_tokens::*;
mod post_tyck;
mod tyck;
pub(crate) use crate::ty::*;
pub(crate) use post_tyck::*;
pub(crate) use tyck::*;
mod prelude;

mod global;
pub use global::*;

use std::path::Path;
use std::sync::Arc;

use ecow::{eco_format, EcoVec};
use lsp_types::Url;
use tinymist_project::LspComputeGraph;
use tinymist_std::{bail, ImmutPath, Result};
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{EntryReader, TaskInputs, WorldDeps};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Func, Value};
use typst::syntax::{FileId, Source};
use typst::World;

use crate::{path_res_to_url, CompilerQueryResponse, SemanticRequest, StatefulRequest};

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
    /// Get file's id by its path
    fn file_id_by_path(&self, path: &Path) -> FileResult<FileId>;

    /// Get the source of a file by file path.
    fn source_by_path(&self, path: &Path) -> FileResult<Source>;

    /// Resolve the uri for a file id.
    fn uri_for_id(&self, fid: FileId) -> FileResult<Url>;

    /// Get all depended file ids of a compilation, inclusively.
    /// Note: must be called after compilation.
    fn depended_files(&self) -> EcoVec<FileId>;

    /// Get all depended paths in file system of a compilation, inclusively.
    /// Note: must be called after compilation.
    fn depended_fs_paths(&self) -> EcoVec<ImmutPath>;
}

impl LspWorldExt for tinymist_project::LspWorld {
    fn file_id_by_path(&self, path: &Path) -> FileResult<FileId> {
        // todo: source in packages
        match self.id_for_path(path) {
            Some(id) => Ok(id),
            None => WorkspaceResolver::file_with_parent_root(path).ok_or_else(|| {
                let reason = eco_format!("invalid path: {path:?}");
                FileError::Other(Some(reason))
            }),
        }
    }

    fn source_by_path(&self, path: &Path) -> FileResult<Source> {
        // todo: source cache
        self.source(self.file_id_by_path(path)?)
    }

    fn uri_for_id(&self, fid: FileId) -> Result<Url, FileError> {
        let res = path_res_to_url(self.path_for_id(fid)?);

        crate::log_debug_ct!("uri_for_id: {fid:?} -> {res:?}");
        res.map_err(|err| FileError::Other(Some(eco_format!("convert to url: {err:?}"))))
    }

    fn depended_files(&self) -> EcoVec<FileId> {
        let mut deps = EcoVec::new();
        self.iter_dependencies(&mut |file_id| {
            deps.push(file_id);
        });
        deps
    }

    fn depended_fs_paths(&self) -> EcoVec<ImmutPath> {
        let mut deps = EcoVec::new();
        self.iter_dependencies(&mut |file_id| {
            if let Ok(path) = self.path_for_id(file_id) {
                deps.push(path.as_path().into());
            }
        });
        deps
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

    /// Runs a stateful query.
    pub fn run_stateful<T: StatefulRequest>(
        self,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> Result<CompilerQueryResponse> {
        let graph = self.snap.clone();
        self.run_analysis(|ctx| query.request(ctx, graph))
            .map(wrapper)
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
        let world = self.snap.world().clone();
        let Some(..) = world.main_id() else {
            log::error!("Project: main file is not set");
            bail!("main file is not set");
        };

        let mut ctx = self.analysis.enter_(world, self.rev_lock);
        Ok(f(&mut ctx))
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

    use crate::syntax::{Expr, RefExpr};
    use crate::tests::*;

    trait ShowExpr {
        fn show_expr(&self, expr: &Expr) -> String;
    }

    impl ShowExpr for Source {
        fn show_expr(&self, node: &Expr) -> String {
            match node {
                Expr::Decl(decl) => {
                    let range = self.range(decl.span()).unwrap_or_default();
                    let fid = if let Some(fid) = decl.file_id() {
                        let vpath = fid.vpath().as_rooted_path();
                        match fid.package() {
                            Some(package) if WorkspaceResolver::is_package_file(fid) => {
                                format!(" in {package:?}{}", unix_slash(vpath))
                            }
                            Some(_) | None => format!(" in {}", unix_slash(vpath)),
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
                    .map(|id| unix_slash(id.vpath().as_rooted_path()))
                    .collect();
                ids.sort();
                ids
            }

            let dependencies = construct_module_dependencies(ctx);

            let mut dependencies = dependencies
                .into_iter()
                .map(|(id, v)| {
                    (
                        unix_slash(id.vpath().as_rooted_path()),
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

    use crate::tests::*;

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
                .map(|pair| (source.range(*pair.0).unwrap_or_default(), pair.1))
                .collect::<Vec<_>>();

            mapping.sort_by(|x, y| {
                x.0.start
                    .cmp(&y.0.start)
                    .then_with(|| x.0.end.cmp(&y.0.end))
            });

            for (range, value) in mapping {
                let ty = Ty::from_types(value.clone().into_iter());
                writeln!(f, "{range:?} -> {ty:?}")?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod post_type_check_tests {

    use insta::with_settings;
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
            let text = node.get().clone().into_text();

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

    use insta::with_settings;
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
            let text = node.get().clone().into_text();

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

    use crate::analysis::{analyze_signature, Signature, SignatureTarget};
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
                writeln!(f, "{} -> {:?}", arg.clone().into_text(), arg_call_info)?;
            }

            Ok(())
        }
    }
}

// #[cfg(test)]
// mod lint_tests {
//     use std::collections::BTreeMap;

//     use crate::tests::*;

//     #[test]
//     fn test() {
//         snapshot_testing("lint", &|ctx, path| {
//             let source = ctx.source_by_path(&path).unwrap();
//             let expr = ctx.expr_stage(&source);

//             let result = tinymist_lint::lint_file(&expr);
//             let result =
// crate::diagnostics::DiagWorker::new(ctx).convert_all(result.iter());
//             let result = result
//                 .into_iter()
//                 .map(|(k, v)| (file_path_(&k), v))
//                 .collect::<BTreeMap<_, _>>();
//             assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
//         });
//     }
// }
