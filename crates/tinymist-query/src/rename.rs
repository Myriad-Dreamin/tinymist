use std::ops::Range;

use lsp_types::{
    DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, RenameFile, TextDocumentEdit,
};
use reflexo::path::{unix_slash, PathClean};
use typst::foundations::{Repr, Str};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    analysis::find_definition,
    find_references,
    prelude::*,
    prepare_renaming,
    syntax::{deref_expr, DerefTarget},
};

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

        prepare_renaming(ctx, &deref_target, &lnk)?;

        match deref_target {
            // todo: abs path
            DerefTarget::ImportPath(path) | DerefTarget::IncludePath(path) => {
                let ref_path_str = path.cast::<ast::Str>()?.get();

                let new_path_str = if !self.new_name.ends_with(".typ") {
                    self.new_name + ".typ"
                } else {
                    self.new_name
                };

                let mut editions: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                let mut document_changes = vec![];

                let def_fid = lnk.def_at?.0;
                let old_path = ctx.path_for_id(def_fid).ok()?;

                let rename_loc = Path::new(ref_path_str.as_str());
                let diff = pathdiff::diff_paths(Path::new(&new_path_str), rename_loc)?;
                if diff.is_absolute() {
                    log::info!("bad rename: absolute path, base: {rename_loc:?}, new: {new_path_str}, diff: {diff:?}");
                    return None;
                }

                let new_path = old_path.join(&diff);

                let old_uri = path_to_url(&old_path).ok()?;
                let new_uri = path_to_url(&new_path).ok()?;

                document_changes.push(lsp_types::DocumentChangeOperation::Op(
                    lsp_types::ResourceOp::Rename(RenameFile {
                        old_uri,
                        new_uri,
                        options: None,
                        annotation_id: None,
                    }),
                ));

                let dep = ctx.module_dependencies().get(&def_fid)?.clone();

                for ref_fid in dep.dependents.iter() {
                    let ref_src = ctx.source_by_id(*ref_fid).ok()?;
                    let uri = ctx.uri_for_id(*ref_fid).ok()?;

                    let Some(import_info) = ctx.import_info(ref_src.clone()) else {
                        continue;
                    };

                    let edits = editions.entry(uri).or_default();
                    for (rng, importing_src) in &import_info.imports {
                        let importing = importing_src.as_ref().map(|s| s.id());
                        if importing.map_or(true, |i| i != def_fid) {
                            continue;
                        }
                        log::debug!("import: {rng:?} -> {importing:?} v.s. {def_fid:?}");
                        rename_importer(ctx, &ref_src, rng.clone(), &diff, edits);
                    }
                }

                // todo: validate: workspace.workspaceEdit.resourceOperations
                for edition in editions.into_iter() {
                    document_changes.push(lsp_types::DocumentChangeOperation::Edit(
                        TextDocumentEdit {
                            text_document: OptionalVersionedTextDocumentIdentifier {
                                uri: edition.0,
                                version: None,
                            },
                            edits: edition.1.into_iter().map(OneOf::Left).collect(),
                        },
                    ));
                }

                Some(WorkspaceEdit {
                    document_changes: Some(DocumentChanges::Operations(document_changes)),
                    ..Default::default()
                })
            }
            _ => {
                let references = find_references(ctx, source.clone(), doc.as_ref(), deref_target)?;

                let mut editions = HashMap::new();

                let (def_fid, _def_range) = lnk.def_at?;
                let def_loc = {
                    let def_source = ctx.source_by_id(def_fid).ok()?;

                    let uri = ctx.uri_for_id(def_fid).ok()?;

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

                Some(WorkspaceEdit {
                    changes: Some(editions),
                    ..Default::default()
                })
            }
        }
    }
}

fn rename_importer(
    ctx: &AnalysisContext,
    src: &Source,
    rng: Range<usize>,
    diff: &Path,
    edits: &mut Vec<TextEdit>,
) -> Option<()> {
    let root = LinkedNode::new(src.root());
    let import_node = root.leaf_at_compat(rng.start + 1).and_then(deref_expr)?;

    let (import_path, has_path_var) = match import_node.cast::<ast::Expr>()? {
        ast::Expr::Import(i) => (i.source(), i.new_name().is_none() && i.imports().is_none()),
        ast::Expr::Include(i) => (i.source(), false),
        _ => return None,
    };

    let new_text = match import_path {
        ast::Expr::Str(s) => {
            let old_str = s.get();
            let old_path = Path::new(old_str.as_str());
            let new_path = old_path.join(diff).clean();
            let new_str = unix_slash(&new_path);

            let path_part = Str::from(new_str).repr();
            let need_alias = new_path.file_name() != old_path.file_name();

            if has_path_var && need_alias {
                let alias = old_path.file_stem()?.to_str()?;
                format!("{path_part} as {alias}")
            } else {
                path_part.to_string()
            }
        }
        _ => return None,
    };

    let import_path_range = import_node.find(import_path.span())?.range();
    let range = ctx.to_lsp_range(import_path_range, src);

    edits.push(TextEdit { range, new_text });

    Some(())
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
