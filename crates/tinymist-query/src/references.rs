use log::debug;

use crate::{
    analysis::{find_definition, IdentNs, SearchCtx},
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
    let node = match deref_target.clone() {
        DerefTarget::VarAccess(node) => node,
        DerefTarget::Callee(node) => node,
        DerefTarget::Label(node) => node,
        // TODO: Cross document reference Ref
        DerefTarget::Ref(node) => node,
        DerefTarget::ImportPath(..) | DerefTarget::IncludePath(..) => {
            return None;
        }
        // todo: reference
        DerefTarget::Normal(..) => {
            return None;
        }
    };

    let mut may_ident = node.cast::<ast::Expr>()?;
    let mut is_ref = false;
    loop {
        match may_ident {
            ast::Expr::Parenthesized(e) => {
                may_ident = e.expr();
            }
            ast::Expr::FieldAccess(e) => {
                may_ident = e.target();
            }
            ast::Expr::MathIdent(..) | ast::Expr::Ident(..) => {
                break;
            }
            ast::Expr::Label(..) | ast::Expr::Ref(..) => {
                is_ref = true;
                break;
            }
            _ => return None,
        }
    }

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
        is_ref,
    };

    if is_ref {
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
    is_ref: bool,
}

impl<'a, 'w> ReferencesWorker<'a, 'w> {
    fn label_root(mut self) -> Option<Vec<LspLocation>> {
        let mut ids = vec![];

        // collect ids first to avoid deadlocks
        self.ctx.ctx.resources.iter_dependencies(&mut |path| {
            if let Ok(ref_fid) = self.ctx.ctx.file_id_by_path(&path) {
                ids.push(ref_fid);
            }
        });

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

        self.push_ident(self.def_fid, &def_source, &uri, def_use.get_refs(def_id));

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
            self.push_ident(ref_fid, &ref_source, &uri, def_use.get_refs(id));

            redefines.push(id);

            if def_use.is_exported(id) {
                self.ctx.push_dependents(ref_fid);
            }
        };

        if self.is_ref {
            // todo: avoid clone by intern names
            let ident_name = self.def_ident.name.clone();
            let ref_idents = def_use.undefined_refs.iter().filter_map(|(ident, ns)| {
                (matches!(ns, IdentNs::Label) && ident.name == ident_name).then_some(ident)
            });

            self.push_ident(ref_fid, &ref_source, &uri, ref_idents);
        }

        Some(())
    }

    fn push_ident<'b>(
        &mut self,
        ref_fid: TypstFileId,
        ref_source: &Source,
        uri: &Url,
        r: impl Iterator<Item = &'b IdentRef>,
    ) {
        self.references.extend(r.map(|r| {
            log::debug!("references: at file: {ref_fid:?}, {r:?}");

            let range = self.ctx.ctx.to_lsp_range(r.range.clone(), ref_source);

            LspLocation {
                uri: uri.clone(),
                range,
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use typst_ts_core::path::unix_slash;

    use super::*;
    use crate::syntax::find_module_level_docs;
    use crate::{tests::*, url_to_path};

    #[test]
    fn test() {
        snapshot_testing("references", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let must_compile = has_test_property(&properties, "compile");
            let doc = if must_compile {
                compile_doc_for_test(ctx)
            } else {
                None
            };

            let request = ReferencesRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(ctx, doc);
            // sort
            let result = result.map(|mut e| {
                e.sort_by(|a, b| match a.range.start.cmp(&b.range.start) {
                    std::cmp::Ordering::Equal => a.range.end.cmp(&b.range.end),
                    e => e,
                });
                e
            });

            let result = result.map(|v| {
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

            assert_snapshot!(JsonRepr::new_pure(result));
        });
    }
}
