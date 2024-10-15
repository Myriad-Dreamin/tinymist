use std::sync::Arc;

use ecow::{eco_format, EcoString};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use tinymist_world::LspWorld;
use typst::foundations::Bytes;
use typst::{
    diag::StrResult,
    syntax::{FileId, VirtualPath},
};

use crate::docs::library;

use super::tidy::*;

/// Kind of a docstring.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub enum DocStringKind {
    /// A docstring for a function.
    Function,
    /// A docstring for a variable.
    Variable,
}

/// Docs about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RawDocs {
    /// Docs about a function.
    #[serde(rename = "func")]
    Function(TidyFuncDocs),
    /// Docs about a variable.
    #[serde(rename = "var")]
    Variable(TidyVarDocs),
    /// Docs about a module.
    #[serde(rename = "module")]
    Module(TidyModuleDocs),
    /// Other kinds of docs.
    #[serde(rename = "plain")]
    Plain(EcoString),
}

impl RawDocs {
    /// Get the markdown representation of the docs.
    pub fn docs(&self) -> &str {
        match self {
            Self::Function(docs) => docs.docs.as_str(),
            Self::Variable(docs) => docs.docs.as_str(),
            Self::Module(docs) => docs.docs.as_str(),
            Self::Plain(docs) => docs.as_str(),
        }
    }
}

// Unfortunately, we have only 65536 possible file ids and we cannot revoke
// them. So we share a global file id for all docs conversion.
static DOCS_CONVERT_ID: std::sync::LazyLock<Mutex<FileId>> = std::sync::LazyLock::new(|| {
    Mutex::new(FileId::new(None, VirtualPath::new("__tinymist_docs__.typ")))
});

pub(crate) fn convert_docs(world: &LspWorld, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: std::sync::LazyLock<Arc<typlite::scopes::Scopes<typlite::value::Value>>> =
        std::sync::LazyLock::new(library::lib);

    let conv_id = DOCS_CONVERT_ID.lock();
    let entry = EntryState::new_rootless(conv_id.vpath().as_rooted_path().into()).unwrap();
    let entry = entry.select_in_workspace(*conv_id);

    let mut w = world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });
    w.map_shadow_by_id(*conv_id, Bytes::from(content.as_bytes().to_owned()))?;
    // todo: bad performance
    w.source_db.take_state();

    let conv = typlite::Typlite::new(Arc::new(w))
        .with_library(DOCS_LIB.clone())
        .annotate_elements(true)
        .convert()
        .map_err(|e| eco_format!("failed to convert to markdown: {e}"))?;

    Ok(conv)
}

pub(crate) fn identify_docs(kind: &str, content: &str) -> StrResult<RawDocs> {
    match kind {
        "function" => identify_func_docs(content).map(RawDocs::Function),
        "variable" => identify_var_docs(content).map(RawDocs::Variable),
        "module" => identify_tidy_module_docs(content).map(RawDocs::Module),
        _ => Err(eco_format!("unknown kind {kind}")),
    }
}
