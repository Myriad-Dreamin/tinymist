use crate::prelude::*;

/// Resolve a file id by its import path.
pub fn resolve_id_by_path(
    world: &dyn World,
    current: TypstFileId,
    import_path: &str,
) -> Option<TypstFileId> {
    if import_path.starts_with('@') {
        let spec = import_path.parse::<PackageSpec>().ok()?;
        // Evaluate the manifest.
        let manifest_id = TypstFileId::new(Some(spec.clone()), VirtualPath::new("typst.toml"));
        let bytes = world.file(manifest_id).ok()?;
        let string = std::str::from_utf8(&bytes).map_err(FileError::from).ok()?;
        let manifest: PackageManifest = toml::from_str(string).ok()?;
        manifest.validate(&spec).ok()?;

        // Evaluate the entry point.
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

/// Find a source instance by its import node.
pub fn find_source_by_expr(
    world: &dyn World,
    current: TypstFileId,
    e: ast::Expr,
) -> Option<Source> {
    // todo: this could be valid: import("path.typ"), where v is parenthesized
    match e {
        ast::Expr::Str(s) => world
            .source(resolve_id_by_path(world, current, s.get().as_str())?)
            .ok(),
        _ => None,
    }
}
