use log::debug;
use typst::syntax::Span;

use crate::{
    analysis::{Definition, SearchCtx},
    prelude::*,
    syntax::{DerefTarget, RefExpr},
    ty::Interned,
};

/// The [`textDocument/references`] request is sent from the client to the
/// server to resolve project-wide references for the symbol denoted by the
/// given text document position.
///
/// [`textDocument/references`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_references
#[derive(Debug, Clone)]
pub struct ReferencesRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
}

impl StatefulRequest for ReferencesRequest {
    type Response = Vec<LspLocation>;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let deref_target = ctx.deref_syntax_at(&source, self.position, 1)?;

        let locations = find_references(ctx, source.clone(), doc.as_ref(), deref_target)?;

        debug!("references: {locations:?}");
        Some(locations)
    }
}

pub(crate) fn find_references(
    ctx: &mut AnalysisContext,
    source: Source,
    doc: Option<&VersionedDocument>,
    target: DerefTarget<'_>,
) -> Option<Vec<LspLocation>> {
    let finding_label = match target {
        DerefTarget::VarAccess(..) | DerefTarget::Callee(..) => false,
        DerefTarget::Label(..) | DerefTarget::Ref(..) => true,
        DerefTarget::ImportPath(..) | DerefTarget::IncludePath(..) | DerefTarget::Normal(..) => {
            return None;
        }
    };

    let def = ctx.definition(source, doc, target)?;

    let worker = ReferencesWorker {
        ctx: ctx.fork_for_search(),
        references: vec![],
        def,
    };

    if finding_label {
        worker.label_root()
    } else {
        // todo: reference of builtin items?
        worker.ident_root()
    }
}

struct ReferencesWorker<'a, 'w> {
    ctx: SearchCtx<'a, 'w>,
    references: Vec<LspLocation>,
    def: Definition,
}

impl<'a, 'w> ReferencesWorker<'a, 'w> {
    fn label_root(mut self) -> Option<Vec<LspLocation>> {
        let mut ids = vec![];

        for dep in self.ctx.ctx.dependencies() {
            if let Ok(ref_fid) = self.ctx.ctx.file_id_by_path(&dep) {
                ids.push(ref_fid);
            }
        }

        for ref_fid in ids {
            self.file(ref_fid)?;
        }

        Some(self.references)
    }

    fn ident_root(mut self) -> Option<Vec<LspLocation>> {
        self.file(self.def.decl.file_id()?);
        while let Some(ref_fid) = self.ctx.worklist.pop() {
            self.file(ref_fid);
        }

        Some(self.references)
    }

    fn file(&mut self, ref_fid: TypstFileId) -> Option<()> {
        log::debug!("references: file: {ref_fid:?}");
        let ref_source = self.ctx.ctx.source_by_id(ref_fid).ok()?;
        let expr_info = self.ctx.ctx.expr_stage(&ref_source);
        let uri = self.ctx.ctx.uri_for_id(ref_fid).ok()?;

        let t = expr_info.get_refs(self.def.decl.clone());
        self.push_idents(&ref_source, &uri, t);

        if expr_info.is_exported(&self.def.decl) {
            self.ctx.push_dependents(ref_fid);
        }

        Some(())
    }

    fn push_idents<'b>(
        &mut self,
        s: &Source,
        u: &Url,
        idents: impl Iterator<Item = (&'b Span, &'b Interned<RefExpr>)>,
    ) {
        self.push_ranges(s, u, idents.map(|e| e.0));
    }

    fn push_ranges<'b>(&mut self, s: &Source, u: &Url, rs: impl Iterator<Item = &'b Span>) {
        self.references.extend(rs.filter_map(|span| {
            // todo: this is not necessary a name span
            let range = self.ctx.ctx.to_lsp_range(s.range(*span)?, s);
            Some(LspLocation {
                uri: u.clone(),
                range,
            })
        }));
    }
}

#[cfg(test)]
mod tests {
    use reflexo::path::unix_slash;

    use super::*;
    use crate::syntax::find_module_level_docs;
    use crate::{tests::*, url_to_path};

    #[test]
    fn test() {
        snapshot_testing("references", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let doc = compile_doc_for_test(ctx, &properties);

            let request = ReferencesRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(ctx, doc);
            let mut result = result.map(|v| {
                v.into_iter()
                    .map(|l| {
                        let fp = unix_slash(&url_to_path(l.uri));
                        let fp = fp.strip_prefix("C:").unwrap_or(&fp);
                        format!(
                            "{fp}@{}:{}:{}:{}",
                            l.range.start.line,
                            l.range.start.character,
                            l.range.end.line,
                            l.range.end.character
                        )
                    })
                    .collect::<Vec<_>>()
            });
            // sort
            if let Some(result) = result.as_mut() {
                result.sort();
            }

            assert_snapshot!(JsonRepr::new_pure(result));
        });
    }
}
