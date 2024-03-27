use crate::prelude::*;

fn resolve_id_by_path(
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

/// Find a source instance by its import path.
pub fn find_source_by_import_path(
    world: &dyn World,
    current: TypstFileId,
    import_path: &str,
) -> Option<Source> {
    world
        .source(resolve_id_by_path(world, current, import_path)?)
        .ok()
}

/// Find a source instance by its import node.
pub fn find_source_by_import(
    world: &dyn World,
    current: TypstFileId,
    import_node: ast::ModuleImport,
) -> Option<Source> {
    // todo: this could be vaild: import("path.typ"), where v is parenthesized
    let v = import_node.source();
    match v {
        ast::Expr::Str(s) => find_source_by_import_path(world, current, s.get().as_str()),
        _ => None,
    }
}

/// Find all static imports in a source.
pub fn find_imports(world: &dyn World, source: &Source) -> EcoVec<TypstFileId> {
    let root = LinkedNode::new(source.root());

    let mut worker = ImportWorker {
        world,
        current: source.id(),
        imports: EcoVec::new(),
    };

    worker.analyze(root);
    let res = worker.imports;

    let mut res: Vec<TypstFileId> = res.into_iter().map(|(id, _)| id).collect();
    res.sort();
    res.dedup();
    res.into_iter().collect()
}

struct ImportWorker<'a> {
    world: &'a dyn World,
    current: TypstFileId,
    imports: EcoVec<(TypstFileId, LinkedNode<'a>)>,
}

impl<'a> ImportWorker<'a> {
    fn analyze(&mut self, node: LinkedNode<'a>) -> Option<()> {
        match node.kind() {
            SyntaxKind::ModuleImport => {
                let i = node.cast::<ast::ModuleImport>().unwrap();
                let src = i.source();
                match src {
                    ast::Expr::Str(s) => {
                        // todo: source in packages
                        let s = s.get();
                        let id = resolve_id_by_path(self.world, self.current, s.as_str())?;

                        self.imports.push((id, node));
                    }
                    // todo: handle dynamic import
                    ast::Expr::FieldAccess(..) | ast::Expr::Ident(..) => {}
                    _ => {}
                }
                return None;
            }
            SyntaxKind::ModuleInclude => {}
            _ => {}
        }
        for child in node.children() {
            self.analyze(child);
        }

        None
    }
}
