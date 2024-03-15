use crate::{find_definition, prelude::*, syntax::get_deref_target, DefinitionLink};
use log::debug;

#[derive(Debug, Clone)]
pub struct PrepareRenameRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

// todo: rename alias
// todo: rename import path?
impl PrepareRenameRequest {
    /// See <https://github.com/microsoft/vscode-go/issues/2714>.
    /// The prepareRename feature is sent before a rename request. If the user
    /// is trying to rename a symbol that should not be renamed (inside a
    /// string or comment, on a builtin identifier, etc.), VSCode won't even
    /// show the rename pop-up.
    pub fn request(
        self,
        ctx: &mut AnalysisContext,
        position_encoding: PositionEncoding,
    ) -> Option<PrepareRenameResponse> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node)?;
        let use_site = deref_target.node().clone();
        let origin_selection_range =
            typst_to_lsp::range(use_site.range(), &source, position_encoding);

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
