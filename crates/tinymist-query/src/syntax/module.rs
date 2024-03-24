use std::{collections::HashMap, path::Path, sync::Once};

use ecow::EcoVec;
use typst::syntax::{FileId as TypstFileId, VirtualPath};

use crate::prelude::AnalysisContext;

use super::find_imports;

pub struct ModuleDependency {
    pub dependencies: EcoVec<TypstFileId>,
    pub dependents: EcoVec<TypstFileId>,
}

pub fn construct_module_dependencies(
    ctx: &mut AnalysisContext,
) -> HashMap<TypstFileId, ModuleDependency> {
    let mut dependencies = HashMap::new();
    let mut dependents = HashMap::new();

    for file_id in ctx.files().clone() {
        let source = match ctx.source_by_id(file_id) {
            Ok(source) => source,
            Err(err) => {
                static WARN_ONCE: Once = Once::new();
                WARN_ONCE.call_once(|| {
                    log::warn!("construct_module_dependencies: {err:?}", err = err);
                });
                continue;
            }
        };

        let file_id = source.id();
        let deps = find_imports(ctx.world(), &source);
        dependencies
            .entry(file_id)
            .or_insert_with(|| ModuleDependency {
                dependencies: deps.clone(),
                dependents: EcoVec::default(),
            });
        for dep in deps {
            dependents
                .entry(dep)
                .or_insert_with(EcoVec::new)
                .push(file_id);
        }
    }

    for (file_id, dependents) in dependents {
        if let Some(dep) = dependencies.get_mut(&file_id) {
            dep.dependents = dependents;
        }
    }

    dependencies
}

pub fn scan_workspace_files(root: &Path) -> Vec<TypstFileId> {
    let mut res = vec![];
    for path in walkdir::WalkDir::new(root).follow_links(false).into_iter() {
        let Ok(de) = path else {
            continue;
        };
        if !de.file_type().is_file() {
            continue;
        }
        if !de
            .path()
            .extension()
            .is_some_and(|e| e == "typ" || e == "typc")
        {
            continue;
        }

        let path = de.path();
        let relative_path = match path.strip_prefix(root) {
            Ok(p) => p,
            Err(err) => {
                log::warn!("failed to strip prefix, path: {path:?}, root: {root:?}: {err}");
                continue;
            }
        };

        res.push(TypstFileId::new(None, VirtualPath::new(relative_path)));
    }

    res
}
