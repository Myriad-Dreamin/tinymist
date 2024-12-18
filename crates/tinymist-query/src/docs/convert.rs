use std::sync::{Arc, LazyLock};

use ecow::{eco_format, EcoString};
use parking_lot::Mutex;
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use typlite::scopes::Scopes;
use typlite::value::Value;
use typlite::TypliteFeat;
use typst::foundations::Bytes;
use typst::{
    diag::StrResult,
    syntax::{FileId, VirtualPath},
};

use crate::analysis::SharedContext;

// Unfortunately, we have only 65536 possible file ids and we cannot revoke
// them. So we share a global file id for all docs conversion.
static DOCS_CONVERT_ID: LazyLock<Mutex<FileId>> =
    LazyLock::new(|| Mutex::new(FileId::new(None, VirtualPath::new("__tinymist_docs__.typ"))));

pub(crate) fn convert_docs(ctx: &SharedContext, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: LazyLock<Arc<Scopes<Value>>> =
        LazyLock::new(|| Arc::new(typlite::library::docstring_lib()));

    let conv_id = DOCS_CONVERT_ID.lock();
    let entry = EntryState::new_rootless(conv_id.vpath().as_rooted_path().into()).unwrap();
    let entry = entry.select_in_workspace(*conv_id);

    let mut w = ctx.world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });
    w.map_shadow_by_id(*conv_id, Bytes::from(content.as_bytes().to_owned()))?;
    // todo: bad performance
    w.source_db.take_state();

    let conv = typlite::Typlite::new(Arc::new(w))
        .with_library(DOCS_LIB.clone())
        .with_feature(TypliteFeat {
            color_theme: Some(ctx.analysis.color_theme),
            annotate_elem: true,
            soft_error: true,
            remove_html: ctx.analysis.remove_html,
            ..Default::default()
        })
        .convert()
        .map_err(|err| eco_format!("failed to convert to markdown: {err}"))?;

    Ok(conv.replace("```example", "```typ"))
}
