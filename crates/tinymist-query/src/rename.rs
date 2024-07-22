use crate::{analysis::find_definition, find_references, prelude::*, validate_renaming_definition};

/// The [`textDocument/rename`] request is sent from the client to the server to
/// ask the server to compute a workspace change so that the client can perform
/// a workspace-wide rename of a symbol.
///
/// [`textDocument/rename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rename
#[derive(Debug, Clone)]
pub struct RenameRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
    /// The new name to rename to.
    pub new_name: String,
}

impl StatefulRequest for RenameRequest {
    type Response = WorkspaceEdit;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let deref_target = ctx.deref_syntax_at(&source, self.position, 1)?;

        let lnk = find_definition(ctx, source.clone(), doc.as_ref(), deref_target.clone())?;

        validate_renaming_definition(&lnk)?;

        let def_use = ctx.def_use(source.clone())?;
        let references = find_references(ctx, def_use, deref_target)?;

        let mut editions = HashMap::new();

        let (fid, _def_range) = lnk.def_at?;

        let def_loc = {
            let def_source = ctx.source_by_id(fid).ok()?;

            let uri = ctx.uri_for_id(fid).ok()?;

            let Some(range) = lnk.name_range else {
                log::warn!("rename: no name range");
                return None;
            };

            LspLocation {
                uri,
                range: ctx.to_lsp_range(range, &def_source),
            }
        };

        for i in (Some(def_loc).into_iter()).chain(references) {
            let uri = i.uri;
            let range = i.range;
            let edits = editions.entry(uri).or_insert_with(Vec::new);
            edits.push(TextEdit {
                range,
                new_text: self.new_name.clone(),
            });
        }

        log::info!("rename editions: {editions:?}");

        // todo: name conflict analysis
        Some(WorkspaceEdit {
            changes: Some(editions),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("rename", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = RenameRequest {
                path: path.clone(),
                position: find_test_position(&source),
                new_name: "new_name".to_string(),
            };

            let mut result = request.request(world, None);
            // sort the edits to make the snapshot stable
            if let Some(r) = result.as_mut().and_then(|r| r.changes.as_mut()) {
                for edits in r.values_mut() {
                    edits.sort_by(|a, b| {
                        a.range
                            .start
                            .cmp(&b.range.start)
                            .then(a.range.end.cmp(&b.range.end))
                    });
                }
            };

            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
