use std::ops::Range;

use log::debug;

use crate::{
    analysis::{find_definition, SearchCtx},
    prelude::*,
    syntax::{DerefTarget, IdentRef},
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
    ctx: &mut AnalysisContext<'_>,
    source: Source,
    document: Option<&VersionedDocument>,
    deref_target: DerefTarget<'_>,
) -> Option<Vec<LspLocation>> {
    let finding_label = match deref_target {
        DerefTarget::VarAccess(..) | DerefTarget::Callee(..) => false,
        DerefTarget::Label(..) | DerefTarget::Ref(..) => true,
        DerefTarget::ImportPath(..) | DerefTarget::IncludePath(..) | DerefTarget::Normal(..) => {
            return None;
        }
    };

    let def = find_definition(ctx, source, document, deref_target)?;

    // todo: reference of builtin items?
    let (def_fid, def_range) = def.def_at?;

    let def_ident = IdentRef {
        name: def.name.clone(),
        range: def_range,
    };

    let def_source = ctx.source_by_id(def_fid).ok()?;
    let root_def_use = ctx.def_use(def_source)?;
    let root_def_id = root_def_use.get_def(def_fid, &def_ident).map(|e| e.0);

    let worker = ReferencesWorker {
        ctx: ctx.fork_for_search(),
        references: vec![],
        def_fid,
        def_ident,
        finding_label,
    };

    if finding_label {
        worker.label_root()
    } else {
        worker.ident_root(root_def_use, root_def_id?)
    }
}

struct ReferencesWorker<'a, 'w> {
    ctx: SearchCtx<'a, 'w>,
    references: Vec<LspLocation>,
    def_fid: TypstFileId,
    def_ident: IdentRef,
    finding_label: bool,
}

impl<'a, 'w> ReferencesWorker<'a, 'w> {
    fn label_root(mut self) -> Option<Vec<LspLocation>> {
        let mut ids = vec![];

        for dep in self.ctx.ctx.resources.dependencies() {
            if let Ok(ref_fid) = self.ctx.ctx.file_id_by_path(&dep) {
                ids.push(ref_fid);
            }
        }

        for ref_fid in ids {
            self.file(ref_fid)?;
        }

        Some(self.references)
    }

    fn ident_root(
        mut self,
        def_use: Arc<crate::analysis::DefUseInfo>,
        def_id: DefId,
    ) -> Option<Vec<LspLocation>> {
        let def_source = self.ctx.ctx.source_by_id(self.def_fid).ok()?;
        let uri = self.ctx.ctx.uri_for_id(self.def_fid).ok()?;

        self.push_idents(&def_source, &uri, def_use.get_refs(def_id));

        if def_use.is_exported(def_id) {
            // Find dependents
            self.ctx.push_dependents(self.def_fid);
            while let Some(ref_fid) = self.ctx.worklist.pop() {
                self.file(ref_fid);
            }
        }

        Some(self.references)
    }

    fn file(&mut self, ref_fid: TypstFileId) -> Option<()> {
        log::debug!("references: file: {ref_fid:?}");
        let ref_source = self.ctx.ctx.source_by_id(ref_fid).ok()?;
        let def_use = self.ctx.ctx.def_use(ref_source.clone())?;
        let uri = self.ctx.ctx.uri_for_id(ref_fid).ok()?;

        let mut redefines = vec![];
        if let Some((id, _def)) = def_use.get_def(self.def_fid, &self.def_ident) {
            self.push_idents(&ref_source, &uri, def_use.get_refs(id));

            redefines.push(id);

            if def_use.is_exported(id) {
                self.ctx.push_dependents(ref_fid);
            }
        };

        // All references are not resolved since static analyzers doesn't know anything
        // about labels (which is working at runtime).
        if self.finding_label {
            let label_refs = def_use.label_refs.get(&self.def_ident.name);
            self.push_ranges(&ref_source, &uri, label_refs.into_iter().flatten());
        }

        Some(())
    }

    fn push_idents<'b>(&mut self, s: &Source, u: &Url, idents: impl Iterator<Item = &'b IdentRef>) {
        self.push_ranges(s, u, idents.map(|e| &e.range));
    }

    fn push_ranges<'b>(&mut self, s: &Source, u: &Url, rs: impl Iterator<Item = &'b Range<usize>>) {
        self.references.extend(rs.map(|rng| {
            log::debug!("references: at file: {s:?}, {rng:?}");

            let range = self.ctx.ctx.to_lsp_range(rng.clone(), s);
            LspLocation {
                uri: u.clone(),
                range,
            }
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
