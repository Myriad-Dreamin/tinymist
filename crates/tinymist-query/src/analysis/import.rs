use std::path::Path;

use log::debug;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind, VirtualPath};
use typst_ts_core::{typst::prelude::EcoVec, TypstFileId};

use crate::prelude::*;

pub fn find_source_by_import_path(
    world: Tracked<'_, dyn World>,
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
    world: Tracked<'_, dyn World>,
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

// todo: bad peformance
pub fn find_imports(
    source: &Source,
    def_id: Option<TypstFileId>,
) -> EcoVec<(VirtualPath, LinkedNode<'_>)> {
    let root = LinkedNode::new(source.root());
    if let Some(def_id) = def_id.as_ref() {
        debug!("find imports for {def_id:?}");
    }

    struct ImportWorker<'a> {
        current: TypstFileId,
        def_id: Option<TypstFileId>,
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
                            let s = s.get();
                            let path = Path::new(s.as_str());
                            let vpath = if path.is_relative() {
                                self.current.vpath().join(path)
                            } else {
                                VirtualPath::new(path)
                            };
                            debug!("found import {vpath:?}");

                            if self.def_id.is_some_and(|e| e.vpath() != &vpath) {
                                return None;
                            }

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

    let mut worker = ImportWorker {
        current: source.id(),
        def_id,
        imports: EcoVec::new(),
    };

    worker.analyze(root);

    worker.imports
}
