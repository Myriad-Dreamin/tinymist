pub mod track_values;
pub use track_values::*;
pub mod lexical_hierarchy;
pub(crate) use lexical_hierarchy::*;
pub mod definition;
pub use definition::*;
pub mod import;
pub use import::*;
pub mod reference;
pub use reference::*;
pub mod def_use;
pub use def_use::*;

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

                let result = get_def_use(source);
                let result = result.as_ref().map(DefUseSnapshot);

                assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
            });
        }

        def_use("lexical_hierarchy");
        def_use("def_use");
    }
}
