use crate::{create_change_annotation, do_rename_file, edits_to_document_changes, prelude::*};

/// Handle [`workspace/willRenameFiles`] request is sent from the client to the
/// server.
///
/// [`workspace/willRenameFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_willRenameFiles
#[derive(Debug, Clone)]
pub struct WillRenameFilesRequest {
    /// rename paths from `left` to `right`
    pub paths: Vec<(PathBuf, PathBuf)>,
}

impl SemanticRequest for WillRenameFilesRequest {
    type Response = WorkspaceEdit;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let mut edits: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        self.paths
            .into_iter()
            .map(|(left, right)| {
                let diff = tinymist_std::path::diff(&right, &left)?;
                log::info!("did rename diff: {diff:?}");
                if diff.is_absolute() {
                    log::info!(
                        "bad rename: absolute path, base: {left:?}, new: {right:?}, diff: {diff:?}"
                    );
                    return None;
                }

                let def_fid = ctx.file_id_by_path(&left).ok()?;
                log::info!("did rename def_fid: {def_fid:?}");

                do_rename_file(ctx, def_fid, diff, &mut edits)
            })
            .collect::<Option<Vec<()>>>()?;
        log::info!("did rename edits: {edits:?}");
        let document_changes = edits_to_document_changes(edits, None);
        if document_changes.is_empty() {
            return None;
        }

        let change_annotations = Some(create_change_annotation(
            "Typst Rename Files",
            true,
            Some("Renaming files should update imports".to_string()),
        ));

        Some(WorkspaceEdit {
            changes: None,
            document_changes: Some(lsp_types::DocumentChanges::Operations(document_changes)),
            change_annotations,
        })
    }
}
