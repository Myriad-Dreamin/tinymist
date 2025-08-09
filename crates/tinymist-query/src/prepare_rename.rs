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

    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response> {
        let doc = graph.snap.success_doc.as_ref();
        let source = ctx.source_by_path(&self.path).ok()?;
        let syntax = ctx.classify_for_decl(&source, self.position)?;
        if bad_syntax(&syntax) {
            return None;
        }

        let origin_selection_range = ctx.to_lsp_range(syntax.node().range(), &source);
        let def = ctx.def_of_syntax(&source, doc, syntax.clone())?;

        let name = prepare_renaming(&syntax, &def)?;

        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: origin_selection_range,
            placeholder: name,
        })
    }
}

fn bad_syntax(syntax: &SyntaxClass) -> bool {
    if matches!(syntax.node().kind(), SyntaxKind::FieldAccess) {
        // todo: rename field access
        log::info!("prepare_rename: field access is not a definition site");
        return true;
    }

    if syntax.contains_error() {
        return true;
    }

    false
}

pub(crate) fn prepare_renaming(syntax: &SyntaxClass, def: &Definition) -> Option<String> {
    if bad_syntax(syntax) {
        return None;
    }

    let def_fid = def.file_id()?;

    if WorkspaceResolver::is_package_file(def_fid) {
        crate::log_debug_ct!(
            "prepare_rename: is in a package {pkg:?}, def: {def:?}",
            pkg = def_fid.package(),
        );
        return None;
    }

    let decl_name = || def.name().clone().to_string();

    use Decl::*;
    match def.decl.as_ref() {
        // Cannot rename headings or blocks
        // LexicalKind::Heading(_) | LexicalKind::Block => None,
        // Cannot rename module star
        // LexicalKind::Mod(Star) => None,
        // Cannot rename expression import
        // LexicalKind::Mod(Module(ModSrc::Expr(..))) => None,
        Var(..) | Label(..) | ContentRef(..) => Some(decl_name()),
        Func(..) | Closure(..) => validate_fn_renaming(def).map(|_| decl_name()),
        Module(..) | ModuleAlias(..) | PathStem(..) | ImportPath(..) | IncludePath(..)
        | ModuleImport(..) => {
            let node = syntax.node().get().clone();
            let path = node.cast::<ast::Str>()?;
            let name = path.get().to_string();
            Some(name)
        }
        // todo: bibkey renaming
        BibEntry(..) => None,
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
            Repr::Closure(..) | Repr::Plugin(..) => return Some(()),
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
        snapshot_testing("rename", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let graph = compile_doc_for_test(ctx, &properties);

            let request = PrepareRenameRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(ctx, graph);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
