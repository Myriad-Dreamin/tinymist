//! Semantic static and dynamic analysis of the source code.

mod bib;
pub(crate) use bib::*;
mod pdg;
use pdg::*;
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
mod tests;
