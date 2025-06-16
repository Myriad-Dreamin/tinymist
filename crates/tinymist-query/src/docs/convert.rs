use std::path::Path;
use std::sync::Arc;

use ecow::{eco_format, EcoString};
use tinymist_std::path::unix_slash;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{EntryReader, EntryState, ShadowApi, TaskInputs};
use typlite::TypliteFeat;
use typst::diag::StrResult;
use typst::foundations::Bytes;
use typst::syntax::FileId;
use typst::World;

use crate::analysis::SharedContext;

pub(crate) fn convert_docs(
    ctx: &SharedContext,
    content: &str,
    source_fid: Option<FileId>,
) -> StrResult<EcoString> {
    let mut entry = ctx.world.entry_state();
    let (contextual_content, import_context) = if let Some(fid) = source_fid {
        let root = ctx
            .world
            .vfs()
            .file_path(fid.join("/"))
            .ok()
            .and_then(|e| e.to_err().ok());
        if let Some(root) = root {
            entry = EntryState::new_workspace(root.into());
        }

        let mut imports = Vec::new();
        if WorkspaceResolver::is_package_file(fid) {
            if let Some(pkg) = fid.package() {
                let pkg_spec = pkg.to_string();
                imports.push(format!("#import {pkg_spec:?}"));
                imports.push(format!("#import {pkg_spec:?}: *"));
            }
        }
        imports.push(format!(
            "#import {:?}: *",
            unix_slash(fid.vpath().as_rooted_path())
        ));
        let imports = imports.join("\n");
        let content_with_import: String = if !imports.is_empty() {
            format!("{imports}\n\n{content}")
        } else {
            content.to_owned()
        };

        (content_with_import, Some(imports))
    } else {
        (content.to_owned(), None)
    };

    let entry = entry.select_in_workspace(Path::new("__tinymist_docs__.typ"));

    let mut w = ctx.world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });

    w.map_shadow_by_id(w.main(), Bytes::from_string(contextual_content))?;
    // todo: bad performance
    w.take_db();

    let conv = typlite::Typlite::new(Arc::new(w))
        .with_feature(TypliteFeat {
            color_theme: Some(ctx.analysis.color_theme),
            annotate_elem: true,
            soft_error: true,
            remove_html: ctx.analysis.remove_html,
            import_context,
            ..Default::default()
        })
        .convert()
        .map_err(|err| eco_format!("failed to convert to markdown: {err}"))?;

    Ok(conv.replace("```example", "```typ"))
}
