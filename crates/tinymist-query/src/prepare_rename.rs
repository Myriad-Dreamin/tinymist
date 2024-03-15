use crate::{find_definition, prelude::*, syntax::get_deref_target, DefinitionLink, SyntaxRequest};
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
impl SyntaxRequest for PrepareRenameRequest {
    type Response = PrepareRenameResponse;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let offset = ctx.to_typst_pos(self.position, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node)?;
        let use_site = deref_target.node().clone();
        let origin_selection_range = ctx.to_lsp_range(use_site.range(), &source);

        let lnk = find_definition(ctx, source.clone(), deref_target)?;
        validate_renaming_definition(&lnk)?;

        debug!("prepare_rename: {}", lnk.name);
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: origin_selection_range,
            placeholder: lnk.name,
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

    if lnk.fid.package().is_some() {
        debug!(
            "prepare_rename: {name} is in a package {pkg:?}",
            name = lnk.name,
            pkg = lnk.fid.package()
        );
        return None;
    }

    Some(())
}
