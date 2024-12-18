use std::str::FromStr;

use reflexo_typst::package::PackageSpec;
use rustc_hash::FxHashSet;

use crate::{adt::interner::Interned, prelude::*};

#[derive(Default)]
pub struct IndexInfo {
    pub(crate) paths: FxHashSet<Interned<str>>,
    pub(crate) packages: FxHashSet<PackageSpec>,
    pub(crate) identifiers: FxHashSet<Interned<str>>,
}

#[comemo::memoize]
pub fn get_index_info(src: &Source) -> Arc<IndexInfo> {
    let root = src.root();
    let mut worker = IndexWorker {
        info: IndexInfo::default(),
    };
    worker.visit(root);
    Arc::new(worker.info)
}

struct IndexWorker {
    info: IndexInfo,
}

impl IndexWorker {
    fn visit(&mut self, node: &SyntaxNode) {
        match node.cast::<ast::Expr>() {
            Some(ast::Expr::Str(path_str)) => {
                if path_str.to_untyped().text().len() > 65536 {
                    // skip long strings
                    return;
                }
                let path_str = path_str.get();

                if path_str.starts_with('@') {
                    let pkg_spec = PackageSpec::from_str(&path_str).ok();
                    if let Some(pkg_spec) = pkg_spec {
                        self.info.identifiers.insert(pkg_spec.name.clone().into());
                        self.info.packages.insert(pkg_spec);
                    }
                    return;
                }
                let path = Path::new(path_str.as_str());
                let name = path.file_name().unwrap_or_default().to_str();
                if let Some(name) = name {
                    self.info.paths.insert(name.into());
                }
            }
            Some(ast::Expr::MathIdent(ident)) => {
                self.info.identifiers.insert(ident.get().into());
            }
            Some(ast::Expr::Ident(ident)) => {
                self.info.identifiers.insert(ident.get().into());
            }
            _ => {}
        }

        for child in node.children() {
            self.visit(child);
        }
    }
}
