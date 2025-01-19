use tinymist_world::vfs::WorkspaceResolver;

use crate::{
    analysis::Definition,
    prelude::*,
    syntax::{Decl, SyntaxClass},
};

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
        ctx: &mut LocalContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let syntax = ctx.classify_for_decl(&source, self.position)?;
        if matches!(syntax.node().kind(), SyntaxKind::FieldAccess) {
            // todo: rename field access
            log::info!("prepare_rename: field access is not a definition site");
            return None;
        }

        let origin_selection_range = ctx.to_lsp_range(syntax.node().range(), &source);
        let def = ctx.def_of_syntax(&source, doc.as_ref(), syntax.clone())?;

        let (name, range) = prepare_renaming(ctx, &syntax, &def)?;

        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: range.unwrap_or(origin_selection_range),
            placeholder: name,
        })
    }
}

pub(crate) fn prepare_renaming(
    ctx: &mut LocalContext,
    deref_target: &SyntaxClass,
    def: &Definition,
) -> Option<(String, Option<LspRange>)> {
    let name = def.name().clone();
    let (def_fid, _def_range) = def.location(ctx.shared()).clone()?;

    if WorkspaceResolver::is_package_file(def_fid) {
        crate::log_debug_ct!(
            "prepare_rename: {name} is in a package {pkg:?}",
            pkg = def_fid.package(),
        );
        return None;
    }

    let var_rename = || Some((name.to_string(), None));

    crate::log_debug_ct!("prepare_rename: {name}");
    use Decl::*;
    match def.decl.as_ref() {
        // Cannot rename headings or blocks
        // LexicalKind::Heading(_) | LexicalKind::Block => None,
        // Cannot rename module star
        // LexicalKind::Mod(Star) => None,
        // Cannot rename expression import
        // LexicalKind::Mod(Module(ModSrc::Expr(..))) => None,
        Var(..) => var_rename(),
        Func(..) | Closure(..) => validate_fn_renaming(def).map(|_| (name.to_string(), None)),
        Module(..) | ModuleAlias(..) | PathStem(..) | ImportPath(..) | IncludePath(..)
        | ModuleImport(..) => {
            let node = deref_target.node().get().clone();
            let path = node.cast::<ast::Str>()?;
            let name = path.get().to_string();
            Some((name, None))
        }
        // todo: label renaming, bibkey renaming
        BibEntry(..) | Label(..) | ContentRef(..) => None,
        ImportAlias(..) | Constant(..) | IdentRef(..) | Import(..) | StrName(..) | Spread(..) => {
            None
        }
        Pattern(..) | Content(..) | Generated(..) | Docs(..) => None,
    }
}

fn validate_fn_renaming(def: &Definition) -> Option<()> {
    use typst::foundations::func::Repr;
    let value = def.value();
    let mut func = match &value {
        None => return Some(()),
        Some(Value::Func(func)) => func,
        Some(..) => {
            log::info!(
                "prepare_rename: not a function on function definition site: {:?}",
                def.term
            );
            return None;
        }
    };
    loop {
        match func.inner() {
            // todo: rename with site
            Repr::With(w) => func = &w.0,
            Repr::Closure(..) => return Some(()),
            // native functions can't be renamed
            Repr::Native(..) | Repr::Element(..) => return None,
        }
    }
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
