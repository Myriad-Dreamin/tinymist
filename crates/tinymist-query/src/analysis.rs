//! Semantic static and dynamic analysis of the source code.

pub mod call;
pub use call::*;
pub mod color_exprs;
pub use color_exprs::*;
pub mod def_use;
pub use def_use::*;
pub mod import;
pub use import::*;
pub mod track_values;
pub use track_values::*;
mod prelude;

mod global;
pub use global::*;

#[cfg(test)]
mod module_tests {
    use reflexo::path::unix_slash;
    use serde_json::json;

    use crate::prelude::*;
    use crate::syntax::module::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("modules", &|ctx, _| {
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
mod matcher_tests {

    use typst::syntax::LinkedNode;

    use crate::{syntax::get_def_target, tests::*};

    #[test]
    fn test() {
        snapshot_testing("match_def", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let node = root.leaf_at(pos).unwrap();

            let result = get_def_target(node).map(|e| format!("{:?}", e.node().range()));
            let result = result.as_deref().unwrap_or("<nil>");

            assert_snapshot!(result);
        });
    }
}

#[cfg(test)]
mod document_tests {

    use crate::syntax::find_document_before;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("docs", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let result = find_document_before(&source, pos);
            let result = result.as_deref().unwrap_or("<nil>");

            assert_snapshot!(result);
        });
    }
}

#[cfg(test)]
mod lexical_hierarchy_tests {
    use def_use::DefUseSnapshot;

    use crate::analysis::def_use;
    // use crate::prelude::*;
    use crate::syntax::lexical_hierarchy;
    use crate::tests::*;

    #[test]
    fn scope() {
        snapshot_testing("lexical_hierarchy", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

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
            snapshot_testing(set, &|ctx, path| {
                let source = ctx.source_by_path(&path).unwrap();

                let result = ctx.def_use(source);
                let result = result.as_deref().map(DefUseSnapshot);

                assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
            });
        }

        def_use("lexical_hierarchy");
        def_use("def_use");
    }
}
