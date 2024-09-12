use crate::{
    analysis::{find_definition, DefinitionLink},
    prelude::*,
};
use log::debug;

/// The [`textDocument/prepareRename`] request is sent from the client to the
/// server to setup and test the validity of a rename operation at a given
/// location.
///
/// [`textDocument/prepareRename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_prepareRename
///
/// # Compatibility
///
/// This request was introduced in specification version 3.12.0.
///
/// See <https://github.com/microsoft/vscode-go/issues/2714>.
/// The prepareRename feature is sent before a rename request. If the user
/// is trying to rename a symbol that should not be renamed (inside a
/// string or comment, on a builtin identifier, etc.), VSCode won't even
/// show the rename pop-up.
#[derive(Debug, Clone)]
pub struct PrepareRenameRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
}

// todo: rename alias
// todo: rename import path?
impl StatefulRequest for PrepareRenameRequest {
    type Response = PrepareRenameResponse;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let deref_target = ctx.deref_syntax_at(&source, self.position, 1)?;
        let origin_selection_range = ctx.to_lsp_range(deref_target.node().range(), &source);

        let lnk = find_definition(ctx, source.clone(), doc.as_ref(), deref_target)?;
        validate_renaming_definition(&lnk)?;

        debug!("prepare_rename: {}", lnk.name);
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: origin_selection_range,
            placeholder: lnk.name.to_string(),
        })
    }
}

pub(crate) fn validate_renaming_definition(lnk: &DefinitionLink) -> Option<()> {
    'check_func: {
        use typst::foundations::func::Repr;
        let mut f = match &lnk.value {
            Some(Value::Func(f)) => f,
            Some(..) => {
                log::info!(
                    "prepare_rename: not a function on function definition site: {:?}",
                    lnk.value
                );
                return None;
            }
            None => {
                break 'check_func;
            }
        };
        loop {
            match f.inner() {
                // native functions can't be renamed
                Repr::Native(..) | Repr::Element(..) => return None,
                // todo: rename with site
                Repr::With(w) => f = &w.0,
                Repr::Closure(..) => break,
            }
        }
    }

    let (fid, _def_range) = lnk.def_at.clone()?;

    if fid.package().is_some() {
        debug!(
            "prepare_rename: {name} is in a package {pkg:?}",
            name = lnk.name,
            pkg = fid.package()
        );
        return None;
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn prepare() {
        snapshot_testing("rename", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = PrepareRenameRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, None);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
