//! Import resolution utilities.

use crate::prelude::*;

/// Resolves a file id by its import path.
pub fn resolve_id_by_path(
    world: &dyn World,
    current: TypstFileId,
    import_path: &str,
) -> Option<TypstFileId> {
    if import_path.starts_with('@') {
        let spec = import_path.parse::<PackageSpec>().ok()?;
        // Evaluates the manifest.
        let manifest_id = TypstFileId::new(Some(spec.clone()), VirtualPath::new("typst.toml"));
        let bytes = world.file(manifest_id).ok()?;
        let string = std::str::from_utf8(&bytes).map_err(FileError::from).ok()?;
        let manifest: PackageManifest = toml::from_str(string).ok()?;
        manifest.validate(&spec).ok()?;

        // Evaluates the entry point.
        return Some(manifest_id.join(&manifest.package.entrypoint));
    }

    let path = Path::new(import_path);
    let vpath = if path.is_relative() {
        current.vpath().join(path)
    } else {
        VirtualPath::new(path)
    };

    Some(TypstFileId::new(current.package().cloned(), vpath))
}

/// Finds a source instance by its import node.
pub fn find_source_by_expr(
    world: &dyn World,
    current: TypstFileId,
    import_source: ast::Expr,
) -> Option<Source> {
    // todo: this could be valid: import("path.typ"), where v is parenthesized
    match import_source {
        ast::Expr::Str(s) => world
            .source(resolve_id_by_path(world, current, s.get().as_str())?)
            .ok(),
        _ => None,
    }
}

/// Casts a node to a single include expression.
pub fn cast_include_expr<'a>(name: &str, node: ast::Expr<'a>) -> Option<ast::Expr<'a>> {
    match node {
        ast::Expr::ModuleInclude(inc) => Some(inc.source()),
        ast::Expr::CodeBlock(code) => {
            let exprs = code.body();
            if exprs.exprs().count() != 1 {
                eprintln!("example function must have a single inclusion: {name}");
                return None;
            }
            cast_include_expr(name, exprs.exprs().next().unwrap())
        }
        _ => {
            eprintln!("example function must have a single inclusion: {name}");
            None
        }
    }
}
