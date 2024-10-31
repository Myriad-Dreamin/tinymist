use std::sync::{Arc, LazyLock};

use ecow::{eco_format, EcoString};
use parking_lot::Mutex;
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use tinymist_world::LspWorld;
use typlite::scopes::Scopes;
use typlite::value::{Value, *};
use typst::foundations::Bytes;
use typst::{
    diag::StrResult,
    syntax::{FileId, VirtualPath},
};

// Unfortunately, we have only 65536 possible file ids and we cannot revoke
// them. So we share a global file id for all docs conversion.
static DOCS_CONVERT_ID: LazyLock<Mutex<FileId>> =
    LazyLock::new(|| Mutex::new(FileId::new(None, VirtualPath::new("__tinymist_docs__.typ"))));

pub(crate) fn convert_docs(world: &LspWorld, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: LazyLock<Arc<Scopes<Value>>> = LazyLock::new(lib);

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

    Ok(conv.replace("```example", "```typ"))
}

pub(super) fn lib() -> Arc<Scopes<Value>> {
    let mut scopes = typlite::library::library();

    // todo: how to import this function correctly?
    scopes.define("example", example as RawFunc);

    Arc::new(scopes)
}

/// Evaluate a `example`.
pub fn example(mut args: Args) -> typlite::Result<Value> {
    let body = get_pos_named!(args, body: Content).0;
    let body = body.trim();
    let ticks = body.chars().take_while(|t| *t == '`').collect::<String>();
    let body = &body[ticks.len()..];
    let body = eco_format!("{ticks}typ{body}");

    Ok(Value::Content(body))
}
