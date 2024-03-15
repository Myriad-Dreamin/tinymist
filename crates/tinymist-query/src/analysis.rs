pub mod def_use;
pub use def_use::*;
pub mod import;
pub use import::*;
pub mod lexical_hierarchy;
pub(crate) use lexical_hierarchy::*;
pub mod matcher;
pub use matcher::*;
pub mod module;
pub use module::*;
pub mod track_values;
pub use track_values::*;

mod global;
pub use global::*;

#[cfg(test)]
mod module_tests {
    use serde_json::json;
    use typst_ts_core::path::unix_slash;
    use typst_ts_core::typst::prelude::EcoVec;

    use crate::analysis::module::*;
    use crate::prelude::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing2("modules", &|ctx, _| {
            fn ids(ids: EcoVec<TypstFileId>) -> Vec<String> {
                let mut ids: Vec<String> = ids
                    .into_iter()
                    .map(|id| unix_slash(id.vpath().as_rooted_path()))
                    .collect();
                ids.sort();
                ids
            }

            let dependencies = construct_module_dependencies(ctx);

            let mut dependencies = dependencies
                .into_iter()
                .map(|(id, v)| {
                    (
                        unix_slash(id.vpath().as_rooted_path()),
                        ids(v.dependencies),
                        ids(v.dependents),
                    )
                })
                .collect::<Vec<_>>();

            dependencies.sort();
            // remove /main.typ
            dependencies.retain(|(p, _, _)| p != "/main.typ");

            let dependencies = dependencies
                .into_iter()
                .map(|(id, deps, dependents)| {
                    let mut mp = serde_json::Map::new();
                    mp.insert("id".to_string(), json!(id));
                    mp.insert("dependencies".to_string(), json!(deps));
                    mp.insert("dependents".to_string(), json!(dependents));
                    json!(mp)
                })
                .collect::<Vec<_>>();

            assert_snapshot!(JsonRepr::new_pure(dependencies));
        });
    }
}

#[cfg(test)]
mod lexical_hierarchy_tests {
    use def_use::get_def_use;
    use def_use::DefUseSnapshot;

    use crate::analysis::def_use;
    use crate::analysis::lexical_hierarchy;
    use crate::prelude::*;
    use crate::tests::*;

    #[test]
    fn scope() {
        snapshot_testing("lexical_hierarchy", &|world, path| {
            let source = get_suitable_source_in_workspace(world, &path).unwrap();

            let result = lexical_hierarchy::get_lexical_hierarchy(
                source,
                lexical_hierarchy::LexicalScopeKind::DefUse,
            );

            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }

    #[test]
    fn test_def_use() {
        fn def_use(set: &str) {
            snapshot_testing(set, &|world, path| {
                let source = get_suitable_source_in_workspace(world, &path).unwrap();

                let result = get_def_use(&mut AnalysisContext::new(world), source);
                let result = result.as_deref().map(DefUseSnapshot);

                assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
            });
        }

        def_use("lexical_hierarchy");
        def_use("def_use");
    }
}
