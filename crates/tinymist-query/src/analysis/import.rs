use std::path::Path;

use log::debug;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind, VirtualPath};
use typst_ts_core::{typst::prelude::EcoVec, TypstFileId};

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
