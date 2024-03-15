use std::{collections::HashMap, sync::Once};

use typst_ts_core::{typst::prelude::EcoVec, TypstFileId};

use super::{find_imports2, AnalysisContext};

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
        let deps = find_imports2(&source);
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
