use lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    RenameFile, TextDocumentEdit,
};
use rustc_hash::FxHashSet;
use tinymist_std::path::{unix_slash, PathClean};
use typst::{
    foundations::{Repr, Str},
    syntax::Span,
};

use crate::{
    analysis::{get_link_exprs, LinkObject, LinkTarget},
    find_references,
    prelude::*,
    prepare_renaming,
    syntax::{first_ancestor_expr, get_index_info, node_ancestors, Decl, RefExpr, SyntaxClass},
    ty::Interned,
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

    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response> {
        let doc = graph.snap.success_doc.as_ref();

        let source = ctx.source_by_path(&self.path).ok()?;
        let syntax = ctx.classify_for_decl(&source, self.position)?;

        let def = ctx.def_of_syntax(&source, doc, syntax.clone())?;

        prepare_renaming(ctx, &syntax, &def)?;

        match syntax {
            // todo: abs path
            SyntaxClass::ImportPath(path) | SyntaxClass::IncludePath(path) => {
                let ref_path_str = path.cast::<ast::Str>()?.get();
                let new_path_str = if !self.new_name.ends_with(".typ") {
                    self.new_name + ".typ"
                } else {
                    self.new_name
                };

                let def_fid = def.location(ctx.shared())?.0;
                // todo: rename in untitled files
                let old_path = ctx.path_for_id(def_fid).ok()?.to_err().ok()?;

                // Because of <https://github.com/Manishearth/pathdiff/issues/8>, we have to clean the path
                // before diff.
                let rename_loc = Path::new(ref_path_str.as_str()).clean();
                let new_path = Path::new(new_path_str.as_str()).clean();
                let diff = pathdiff::diff_paths(&new_path, &rename_loc)?;
                if diff.is_absolute() {
                    log::info!("bad rename: absolute path, base: {rename_loc:?}, new: {new_path:?}, diff: {diff:?}");
                    return None;
                }

                let new_path = old_path.join(&diff).clean();

                let old_uri = path_to_url(&old_path).ok()?;
                let new_uri = path_to_url(&new_path).ok()?;

                let mut edits: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                do_rename_file(ctx, def_fid, diff, &mut edits);

                let mut document_changes = edits_to_document_changes(edits);

                document_changes.push(lsp_types::DocumentChangeOperation::Op(
                    lsp_types::ResourceOp::Rename(RenameFile {
                        old_uri,
                        new_uri,
                        options: None,
                        annotation_id: None,
                    }),
                ));

                // todo: validate: workspace.workspaceEdit.resourceOperations
                Some(WorkspaceEdit {
                    document_changes: Some(DocumentChanges::Operations(document_changes)),
                    ..Default::default()
                })
            }
            _ => {
                let references = find_references(ctx, &source, doc, syntax)?;

                let mut edits = HashMap::new();

                for loc in references {
                    let uri = loc.uri;
                    let range = loc.range;
                    let edits = edits.entry(uri).or_insert_with(Vec::new);
                    edits.push(TextEdit {
                        range,
                        new_text: self.new_name.clone(),
                    });
                }

                log::info!("rename edits: {edits:?}");

                Some(WorkspaceEdit {
                    changes: Some(edits),
                    ..Default::default()
                })
            }
        }
    }
}

pub(crate) fn do_rename_file(
    ctx: &mut LocalContext,
    def_fid: TypstFileId,
    diff: PathBuf,
    edits: &mut HashMap<Url, Vec<TextEdit>>,
) -> Option<()> {
    let def_path = def_fid
        .vpath()
        .as_rooted_path()
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .into();
    let mut ctx = RenameFileWorker {
        ctx,
        def_fid,
        def_path,
        diff,
        inserted: FxHashSet::default(),
    };
    ctx.work(edits)
}

struct RenameFileWorker<'a> {
    ctx: &'a mut LocalContext,
    def_fid: TypstFileId,
    def_path: Interned<str>,
    diff: PathBuf,
    inserted: FxHashSet<Span>,
}

impl RenameFileWorker<'_> {
    pub(crate) fn work(&mut self, edits: &mut HashMap<Url, Vec<TextEdit>>) -> Option<()> {
        let dep = self.ctx.module_dependencies().get(&self.def_fid).cloned();
        if let Some(dep) = dep {
            for ref_fid in dep.dependents.iter() {
                self.refs_in_file(*ref_fid, edits);
            }
        }

        for ref_fid in self.ctx.source_files().clone() {
            self.links_in_file(ref_fid, edits);
        }

        Some(())
    }

    fn refs_in_file(
        &mut self,
        ref_fid: TypstFileId,
        edits: &mut HashMap<Url, Vec<TextEdit>>,
    ) -> Option<()> {
        let ref_src = self.ctx.source_by_id(ref_fid).ok()?;
        let uri = self.ctx.uri_for_id(ref_fid).ok()?;

        let import_info = self.ctx.expr_stage(&ref_src);

        let edits = edits.entry(uri).or_default();
        for (span, r) in &import_info.resolves {
            if !matches!(
                r.decl.as_ref(),
                Decl::ImportPath(..) | Decl::IncludePath(..) | Decl::PathStem(..)
            ) {
                continue;
            }

            if let Some(edit) = self.rename_module_path(*span, r, &ref_src) {
                edits.push(edit);
            }
        }

        Some(())
    }

    fn links_in_file(
        &mut self,
        ref_fid: TypstFileId,
        edits: &mut HashMap<Url, Vec<TextEdit>>,
    ) -> Option<()> {
        let ref_src = self.ctx.source_by_id(ref_fid).ok()?;

        let index = get_index_info(&ref_src);
        if !index.paths.contains(&self.def_path) {
            return Some(());
        }

        let uri = self.ctx.uri_for_id(ref_fid).ok()?;

        let link_info = get_link_exprs(&ref_src);
        let root = LinkedNode::new(ref_src.root());
        let edits = edits.entry(uri).or_default();
        for obj in &link_info.objects {
            if !matches!(&obj.target,
                LinkTarget::Path(file_id, _) if *file_id == self.def_fid
            ) {
                continue;
            }
            if let Some(edit) = self.rename_resource_path(obj, &root, &ref_src) {
                edits.push(edit);
            }
        }

        Some(())
    }

    fn rename_resource_path(
        &mut self,
        obj: &LinkObject,
        root: &LinkedNode,
        src: &Source,
    ) -> Option<TextEdit> {
        let r = root.find(obj.span)?;
        self.rename_path_expr(r.clone(), r.cast()?, src, false)
    }

    fn rename_module_path(&mut self, span: Span, r: &RefExpr, src: &Source) -> Option<TextEdit> {
        let importing = r.root.as_ref()?.file_id();

        if importing != Some(self.def_fid) {
            return None;
        }
        crate::log_debug_ct!("import: {span:?} -> {importing:?} v.s. {:?}", self.def_fid);
        // rename_importer(self.ctx, &ref_src, *span, &self.diff, edits);

        let root = LinkedNode::new(src.root());
        let import_node = root.find(span).and_then(first_ancestor_expr)?;
        let (import_path, has_path_var) = node_ancestors(&import_node).find_map(|import_node| {
            match import_node.cast::<ast::Expr>()? {
                ast::Expr::Import(import) => Some((
                    import.source(),
                    import.new_name().is_none() && import.imports().is_none(),
                )),
                ast::Expr::Include(include) => Some((include.source(), false)),
                _ => None,
            }
        })?;

        self.rename_path_expr(import_node.clone(), import_path, src, has_path_var)
    }

    fn rename_path_expr(
        &mut self,
        node: LinkedNode,
        path: ast::Expr,
        src: &Source,
        has_path_var: bool,
    ) -> Option<TextEdit> {
        let new_text = match path {
            ast::Expr::Str(s) => {
                if !self.inserted.insert(s.span()) {
                    return None;
                }

                let old_str = s.get();
                let old_path = Path::new(old_str.as_str());
                let new_path = old_path.join(&self.diff).clean();
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

        let import_path_range = node.find(path.span())?.range();
        let range = self.ctx.to_lsp_range(import_path_range, src);

        Some(TextEdit { range, new_text })
    }
}

pub(crate) fn edits_to_document_changes(
    edits: HashMap<Url, Vec<TextEdit>>,
) -> Vec<DocumentChangeOperation> {
    let mut document_changes = vec![];

    for (uri, edits) in edits {
        document_changes.push(lsp_types::DocumentChangeOperation::Edit(TextDocumentEdit {
            text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
            edits: edits.into_iter().map(OneOf::Left).collect(),
        }));
    }

    document_changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("rename", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = RenameRequest {
                path: path.clone(),
                position: find_test_position(&source),
                new_name: "new_name".to_string(),
            };
            let snap = WorldComputeGraph::from_world(ctx.world.clone());

            let mut result = request.request(ctx, snap);
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
