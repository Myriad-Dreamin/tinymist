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
}
