use std::path::Path;
use std::sync::Arc;

use ecow::{eco_format, EcoString};
use tinymist_world::package::PackageRegistry;
use tinymist_world::vfs::{WorkspaceResolution, WorkspaceResolver};
use tinymist_world::{EntryReader, ShadowApi, TaskInputs};
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
    let entry = ctx.world.entry_state();
    let entry = entry.select_in_workspace(Path::new("__tinymist_docs__.typ"));

    let mut w = ctx.world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });

    let (contextual_content, import_context) = if let Some(fid) = source_fid {
        let import_path = get_import_path_for_file(ctx, fid)?;
        if let Some(path) = import_path {
            let mut imports = Vec::new();
            // if let Some(pkg) = fid.package() {
            //     // THIS SEEMS DON'T WORK
            //     // TODO: This cause an error if working dir does not include `typst.toml`.
            //     imports.push(format!("#import \"{}\": *", pkg));
            // }
            imports.push(format!("#import \"{path}\": *"));
            let imports = imports.join("\n");
            let content_with_import: String = if !imports.is_empty() {
                format!("{imports}\n\n{content}")
            } else {
                content.to_owned()
            };

            (content_with_import, Some(imports))
        } else {
            (content.to_owned(), None)
        }
    } else {
        (content.to_owned(), None)
    };

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

fn get_import_path_for_file(ctx: &SharedContext, source_fid: FileId) -> StrResult<Option<String>> {
    let resolver_result = if let Ok(resolution) = WorkspaceResolver::resolve(source_fid) {
        match resolution {
            WorkspaceResolution::Workspace(workspace_id)
            | WorkspaceResolution::UntitledRooted(workspace_id) => Ok(Some(workspace_id.path())),
            WorkspaceResolution::Package => {
                if let Some(pkg) = source_fid.package() {
                    ctx.world
                        .registry
                        .resolve(pkg)
                        .map(Some)
                        .map_err(|e| eco_format!("failed to resolve package root: {}", e))
                } else {
                    Ok(None)
                }
            }
            WorkspaceResolution::Rootless => Ok(None),
        }
    } else {
        Ok(None)
    };

    match resolver_result {
        Ok(Some(_)) => {
            let vpath = source_fid.vpath();
            let path = vpath.as_rooted_path();
            let path_str = path.to_string_lossy();
            Ok(Some(path_str.to_string()))
        }
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}
