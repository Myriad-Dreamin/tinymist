use std::sync::Once;

use super::find_imports;
use crate::prelude::*;

/// The dependency information of a module (file).
pub struct ModuleDependency {
    /// The dependencies of this module.
    pub dependencies: EcoVec<TypstFileId>,
    /// The dependents of this module.
    pub dependents: EcoVec<TypstFileId>,
}

/// Construct the module dependencies of the given context.
///
/// It will scan all the files in the context, using [`AnalysisContext::files`],
/// and find the dependencies and dependents of each file.
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

/// Scan the files in the workspace and return the file ids.
///
/// Note: this function will touch the physical file system.
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
