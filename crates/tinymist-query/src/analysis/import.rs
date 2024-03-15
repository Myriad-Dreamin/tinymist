use std::path::Path;

use log::debug;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind, VirtualPath};
use typst_ts_core::{typst::prelude::EcoVec, TypstFileId};

use crate::prelude::*;

pub fn find_source_by_import_path(
    world: &dyn World,
    current: TypstFileId,
    import_path: &str,
) -> Option<Source> {
    if import_path.starts_with('@') {
        // todo: import from package
        return None;
    }

    let path = Path::new(import_path);
    let vpath = if path.is_relative() {
        current.vpath().join(path)
    } else {
        VirtualPath::new(path)
    };

    let id = TypstFileId::new(current.package().cloned(), vpath);
    world.source(id).ok()
}

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

#[comemo::memoize]
pub fn find_imports(source: &Source) -> EcoVec<TypstFileId> {
    let root = LinkedNode::new(source.root());

    let mut worker = ImportWorker {
        current: source.id(),
        imports: EcoVec::new(),
    };

    worker.analyze(root);
    let res = worker.imports;

    let mut res: Vec<TypstFileId> = res
        .into_iter()
        .map(|(vpath, _)| TypstFileId::new(None, vpath))
        .collect();
    res.sort();
    res.dedup();
    res.into_iter().collect()
}

struct ImportWorker<'a> {
    current: TypstFileId,
    imports: EcoVec<(VirtualPath, LinkedNode<'a>)>,
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
                        let path = Path::new(s.as_str());
                        let vpath = if path.is_relative() {
                            self.current.vpath().join(path)
                        } else {
                            VirtualPath::new(path)
                        };
                        debug!("found import {vpath:?}");

                        self.imports.push((vpath, node));
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
