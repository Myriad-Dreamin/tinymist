use log::debug;

use crate::{
    analysis::{find_definition, Definition},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct PrepareRenameRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl PrepareRenameRequest {
    /// See <https://github.com/microsoft/vscode-go/issues/2714>.
    /// The prepareRename feature is sent before a rename request. If the user
    /// is trying to rename a symbol that should not be renamed (inside a
    /// string or comment, on a builtin identifier, etc.), VSCode won't even
    /// show the rename pop-up.
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<PrepareRenameResponse> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let typst_offset = lsp_to_typst::position(self.position, position_encoding, &source)?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset)?;

        let Definition::Func(func) = find_definition(world, ast_node)?;

        use typst::foundations::func::Repr;
        let mut f = func.value.clone();
        loop {
            match f.inner() {
                // native functions can't be renamed
                Repr::Native(..) | Repr::Element(..) => return None,
                // todo: rename with site
                Repr::With(w) => f = w.0.clone(),
                Repr::Closure(..) => break,
            }
        }

        // todo: unwrap parentheses
        let ident = match func.use_site.kind() {
            SyntaxKind::Ident | SyntaxKind::MathIdent => func.use_site.text(),
            _ => return None,
        };
        debug!("prepare_rename: {ident}");

        let id = func.span.id()?;
        if id.package().is_some() {
            debug!(
                "prepare_rename: {ident} is in a package {pkg:?}",
                pkg = id.package()
            );
            return None;
        }

        let origin_selection_range =
            typst_to_lsp::range(func.use_site.range(), &source, position_encoding);

        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: origin_selection_range,
            placeholder: ident.to_string(),
        })
    }
}
