use std::path::Path;
use std::sync::Arc;

use ecow::{EcoString, eco_format};
use tinymist_std::path::unix_slash;
use tinymist_world::diag::print_diagnostics_to_string;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{
    DiagnosticFormat, EntryReader, EntryState, ShadowApi, SourceWorld, TaskInputs,
};
use typlite::{Format, TypliteFeat};
use typst::World;
use typst::diag::StrResult;
use typst::foundations::Bytes;
use typst::syntax::FileId;

use crate::analysis::SharedContext;

pub(crate) fn convert_docs(
    ctx: &SharedContext,
    content: &str,
    source_fid: Option<FileId>,
) -> StrResult<EcoString> {
    let mut entry = ctx.world.entry_state();
    let import_context = source_fid.map(|fid| {
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
        if WorkspaceResolver::is_package_file(fid)
            && let Some(pkg) = fid.package()
        {
            let pkg_spec = pkg.to_string();
            imports.push(format!("#import {pkg_spec:?}"));
            imports.push(format!("#import {pkg_spec:?}: *"));
        }
        imports.push(format!(
            "#import {:?}: *",
            unix_slash(fid.vpath().as_rooted_path())
        ));
        imports.join("; ")
    });
    let feat = TypliteFeat {
        color_theme: Some(ctx.analysis.color_theme),
        annotate_elem: true,
        soft_error: true,
        remove_html: ctx.analysis.remove_html,
        import_context,
        ..Default::default()
    };

    let entry = entry.select_in_workspace(Path::new("__tinymist_docs__.typ"));

    let mut w = ctx.world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });

    // todo: bad performance: content.to_owned()
    w.map_shadow_by_id(w.main(), Bytes::from_string(content.to_owned()))?;
    // todo: bad performance
    w.take_db();
    let w = feat
        .prepare_world(&w, Format::Md)
        .map_err(|e| eco_format!("failed to prepare world: {e}"))?;

    let w = Arc::new(w);
    let res = typlite::Typlite::new(w.clone())
        .with_feature(feat)
        .convert();
    let conv = print_diag_or_error(w.as_ref(), res)?;

    Ok(conv.replace("```example", "```typ"))
}

fn print_diag_or_error<T>(
    world: &impl SourceWorld,
    result: tinymist_std::Result<T>,
) -> StrResult<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                return Err(print_diagnostics_to_string(
                    world,
                    diagnostics.iter(),
                    DiagnosticFormat::Human,
                )?);
            }

            Err(eco_format!("failed to convert docs: {err}"))
        }
    }
}
