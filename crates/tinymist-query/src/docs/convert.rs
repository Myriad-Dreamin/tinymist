use std::path::Path;
use std::sync::{Arc, LazyLock};

use ecow::{eco_format, EcoString};
use reflexo_typst::EntryReader;
use tinymist_world::base::ShadowApi;
use tinymist_world::TaskInputs;
use typlite::scopes::Scopes;
use typlite::value::Value;
use typlite::TypliteFeat;
use typst::diag::StrResult;
use typst::foundations::Bytes;
use typst::World;

use crate::analysis::SharedContext;

pub(crate) fn convert_docs(ctx: &SharedContext, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: LazyLock<Arc<Scopes<Value>>> =
        LazyLock::new(|| Arc::new(typlite::library::docstring_lib()));

    let entry = ctx.world.entry_state();
    let entry = entry.select_in_workspace(Path::new("__tinymist_docs__.typ"));

    let mut w = ctx.world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });
    w.map_shadow_by_id(w.main(), Bytes::from(content.as_bytes().to_owned()))?;
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
