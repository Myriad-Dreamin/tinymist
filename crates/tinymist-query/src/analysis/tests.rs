#[cfg(test)]
mod matcher_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::{syntax::classify_def, tests::*};

    #[test]
    fn test() {
        snapshot_testing_at("..", "match_def", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos).unwrap();

            let snap = classify_def(node).map(|def| format!("{:?}", def.node().range()));
            let snap = snap.as_deref().unwrap_or("<nil>");

            assert_snapshot!(snap);
        });
    }
}

#[cfg(test)]
mod expr_tests {

    use rayon::ThreadPoolBuilder;
    use tinymist_std::path::unix_slash;
    use tinymist_world::{ShadowApi, vfs::WorkspaceResolver};
    use typst::{foundations::Bytes, syntax::Source};
    use typst_shim::syntax::{RootedPathExt, VirtualPathExt, source_range};

    use crate::analysis::Analysis;
    use crate::syntax::{Expr, RefExpr};
    use crate::tests::*;

    fn query_count(analysis: &Analysis, query: &str) -> u64 {
        analysis
            .report_query_stats_json()
            .into_iter()
            .find(|stat| stat.file.is_none() && stat.query == query)
            .map_or(0, |stat| stat.count)
    }

    fn expr_stage_count(analysis: &Analysis) -> u64 {
        query_count(analysis, "expr_stage")
    }

    trait ShowExpr {
        fn show_expr(&self, expr: &Expr) -> String;
    }

    impl ShowExpr for Source {
        fn show_expr(&self, node: &Expr) -> String {
            match node {
                Expr::Decl(decl) => {
                    let range = source_range(self, decl.span()).unwrap_or_default();
                    let fid = if let Some(fid) = decl.file_id() {
                        if WorkspaceResolver::is_package_file(fid) {
                            let package = fid.package_compat().expect("package file");
                            format!(
                                " in {package:?}{}",
                                unix_slash(fid.vpath().as_rooted_path_compat())
                            )
                        } else {
                            format!(" in {}", unix_slash(fid.vpath().as_rooted_path_compat()))
                        }
                    } else {
                        "".to_string()
                    };
                    format!("{decl:?}@{range:?}{fid}")
                }
                _ => format!("{node}"),
            }
        }
    }

    #[test]
    fn cached_expr_stage_is_not_counted() {
        const SOURCE: &str = "#let answer = 42";

        run_with_sources(SOURCE, |verse, path| {
            let analysis = Analysis::default();
            let first_revision = {
                let mut world = verse.snapshot();
                world.set_is_compiling(false);
                let ctx = analysis.enter(WorldComputeGraph::from_world(world));
                let source = ctx.source_by_path(&path).unwrap();
                let shared = ctx.shared_();

                assert_eq!(expr_stage_count(&analysis), 0);
                let first = shared.expr_stage(&source);
                assert_eq!(expr_stage_count(&analysis), 1);

                // This call reuses the completed OnceLock in the current revision.
                shared.expr_stage(&source);
                assert_eq!(expr_stage_count(&analysis), 1);

                assert_eq!(first.fid, source.id());
                ctx.revision()
            };

            // Advance the world revision without changing the source. The new
            // revision must validate and reuse the previous expression result.
            verse.increment_revision(|revision| revision.flush());
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            assert_ne!(ctx.revision(), first_revision);

            let source = ctx.source_by_path(&path).unwrap();
            ctx.shared_().expr_stage(&source);
            assert_eq!(expr_stage_count(&analysis), 1);
        });
    }

    #[test]
    fn cyclic_component_is_analyzed_once_for_parallel_roots() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": beta
#let alpha = beta
-----
// path: b.typ
#import "a.typ": alpha
#let beta = alpha
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            let root = entry.parent().expect("entry must have a parent");
            let source_a = ctx.source_by_path(&root.join("a.typ")).unwrap();
            let source_b = ctx.source_by_path(&root.join("b.typ")).unwrap();
            let shared = ctx.shared_();
            let pool = ThreadPoolBuilder::new()
                .num_threads(2)
                .build()
                .expect("two-thread component test pool must initialize");

            let (expr_a, expr_b) = pool.install(|| {
                rayon::join(
                    || shared.expr_stage(&source_a),
                    || shared.expr_stage(&source_b),
                )
            });
            assert_eq!(expr_a.fid, source_a.id());
            assert_eq!(expr_b.fid, source_b.id());
            assert_eq!(expr_stage_count(&analysis), 2);

            let (type_a, type_b) = pool.install(|| {
                rayon::join(
                    || shared.type_check(&source_a),
                    || shared.type_check(&source_b),
                )
            });
            assert!(type_a.valid);
            assert!(type_b.valid);
            assert_eq!(query_count(&analysis, "type_check"), 2);

            shared.expr_stage(&source_a);
            shared.expr_stage(&source_b);
            shared.type_check(&source_a);
            shared.type_check(&source_b);
            assert_eq!(expr_stage_count(&analysis), 2);
            assert_eq!(query_count(&analysis, "type_check"), 2);
        });
    }

    #[test]
    fn cyclic_component_invalidates_expression_history_as_a_batch() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": beta
#let alpha = beta
-----
// path: b.typ
#import "a.typ": *
#let beta = alpha
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let root = entry.parent().expect("entry must have a parent");
            let a_path = root.join("a.typ");
            let b_path = root.join("b.typ");
            let analyze = |verse: &LspUniverse| {
                let mut world = verse.snapshot();
                world.set_is_compiling(false);
                let ctx = analysis.enter(WorldComputeGraph::from_world(world));
                let source_a = ctx.source_by_path(&a_path).unwrap();
                let source_b = ctx.source_by_path(&b_path).unwrap();
                let shared = ctx.shared_();
                shared.expr_stage(&source_a);
                shared.expr_stage(&source_b)
            };

            let first_b = analyze(verse);
            assert_eq!(expr_stage_count(&analysis), 2);
            assert!(first_b.exports.keys().any(|name| name.as_ref() == "alpha"));
            assert!(first_b.exports.keys().any(|name| name.as_ref() == "beta"));

            verse.increment_revision(|revision| revision.flush());
            let cached_b = analyze(verse);
            assert_eq!(expr_stage_count(&analysis), 2);
            assert_eq!(cached_b.revision, first_b.revision);

            // Keep the A -> B -> A component intact while changing only A's
            // local declaration. B's source is unchanged, but its wildcard
            // interface and expression result both depend on A.
            verse
                .map_shadow(
                    &a_path,
                    Bytes::from_string("#import \"b.typ\": beta\n#let renamed = beta"),
                )
                .unwrap();

            let second_b = analyze(verse);
            assert_eq!(expr_stage_count(&analysis), 4);
            assert_ne!(first_b.revision, second_b.revision);
            assert!(!second_b.exports.keys().any(|name| name.as_ref() == "alpha"));
            assert!(
                second_b
                    .exports
                    .keys()
                    .any(|name| name.as_ref() == "renamed")
            );
            assert!(second_b.exports.keys().any(|name| name.as_ref() == "beta"));
        });
    }

    #[test]
    fn cyclic_wildcard_declarations_are_sealed_as_a_batch() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": *
#let alpha = 1
-----
// path: b.typ
#import "a.typ": *
#let beta = 2
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            let root = entry.parent().expect("entry must have a parent");
            let source_a = ctx.source_by_path(&root.join("a.typ")).unwrap();
            let source_b = ctx.source_by_path(&root.join("b.typ")).unwrap();
            let shared = ctx.shared_();

            let expr_a = shared.expr_stage(&source_a);
            let expr_b = shared.expr_stage(&source_b);
            for info in [&expr_a, &expr_b] {
                assert!(info.exports.keys().any(|name| name.as_ref() == "alpha"));
                assert!(info.exports.keys().any(|name| name.as_ref() == "beta"));
            }
            assert_eq!(expr_stage_count(&analysis), 2);
            assert!(shared.type_check(&source_a).valid);
            assert!(shared.type_check(&source_b).valid);
            assert_eq!(query_count(&analysis, "type_check"), 2);
        });
    }

    #[test]
    fn cyclic_wildcard_declarations_reach_a_three_member_fixed_point() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": *
#let alpha = 1
-----
// path: b.typ
#import "c.typ": *
#let beta = 2
-----
// path: c.typ
#import "a.typ": *
#let gamma = 3
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            let root = entry.parent().expect("entry must have a parent");
            let shared = ctx.shared_();

            for name in ["a.typ", "b.typ", "c.typ"] {
                let source = ctx.source_by_path(&root.join(name)).unwrap();
                let info = shared.expr_stage(&source);
                for export in ["alpha", "beta", "gamma"] {
                    assert!(
                        info.exports.keys().any(|name| name.as_ref() == export),
                        "{name} is missing re-exported declaration {export}"
                    );
                }
            }
            assert_eq!(expr_stage_count(&analysis), 3);
        });
    }

    #[test]
    fn cyclic_wildcard_declarations_preserve_source_order_shadowing() {
        const SOURCES: &str = r#"
// path: a.typ
#let x = 1
#import "b.typ": *
-----
// path: b.typ
#import "a.typ": *
#let x = 2
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            let root = entry.parent().expect("entry must have a parent");
            let source_a = ctx.source_by_path(&root.join("a.typ")).unwrap();
            let source_b = ctx.source_by_path(&root.join("b.typ")).unwrap();
            let shared = ctx.shared_();

            for info in [shared.expr_stage(&source_a), shared.expr_stage(&source_b)] {
                let (_, binding) = info
                    .exports
                    .iter()
                    .find(|(name, _)| name.as_ref() == "x")
                    .expect("x must be exported");
                let Expr::Decl(binding) = binding else {
                    panic!("x must resolve to B's final local declaration: {binding:?}");
                };
                assert_eq!(binding.file_id(), Some(source_b.id()));
            }
            assert_eq!(expr_stage_count(&analysis), 2);
        });
    }

    #[test]
    fn external_export_change_invalidates_the_whole_cyclic_expression_batch() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": beta
#let alpha = beta
-----
// path: b.typ
#import "a.typ": alpha
#import "c.typ": *
#let beta = alpha
-----
// path: c.typ
#let downstream = 1
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let root = entry.parent().expect("entry must have a parent");
            let a_path = root.join("a.typ");
            let b_path = root.join("b.typ");
            let c_path = root.join("c.typ");
            let analyze = |verse: &LspUniverse| {
                let mut world = verse.snapshot();
                world.set_is_compiling(false);
                let ctx = analysis.enter(WorldComputeGraph::from_world(world));
                let source_a = ctx.source_by_path(&a_path).unwrap();
                let source_b = ctx.source_by_path(&b_path).unwrap();
                let shared = ctx.shared_();
                (shared.expr_stage(&source_a), shared.expr_stage(&source_b))
            };

            let (first_a, first_b) = analyze(verse);
            assert_eq!(expr_stage_count(&analysis), 3);
            assert!(
                first_b
                    .exports
                    .keys()
                    .any(|name| name.as_ref() == "downstream")
            );

            verse
                .map_shadow(&c_path, Bytes::from_string("#let changed = 2"))
                .unwrap();
            let (second_a, second_b) = analyze(verse);
            assert_eq!(expr_stage_count(&analysis), 6);
            assert_ne!(first_a.revision, second_a.revision);
            assert_ne!(first_b.revision, second_b.revision);
            assert!(
                second_b
                    .exports
                    .keys()
                    .any(|name| name.as_ref() == "changed")
            );
            assert!(
                !second_b
                    .exports
                    .keys()
                    .any(|name| name.as_ref() == "downstream")
            );
        });
    }

    #[test]
    fn dependency_admission_covers_assignment_path_resolution() {
        const SOURCES: &str = r#"
// path: a.typ
#let path = "b.typ"
#{
  path = "c.typ"
  import path
}
-----
// path: b.typ
#let value = 1
-----
// path: c.typ
#let value = 2
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let mut world = verse.snapshot();
            world.set_is_compiling(false);
            let ctx = analysis.enter(WorldComputeGraph::from_world(world));
            let source = ctx.source_by_path(&entry).unwrap();
            let info = ctx.shared_().expr_stage(&source);
            assert_eq!(info.fid, source.id());
        });
    }

    #[test]
    fn type_component_cache_tracks_transitive_expression_inputs() {
        const SOURCES: &str = r#"
// path: b.typ
#let value = 1
-----
// path: a.typ
#import "b.typ": value
#let result = value
"#;

        run_with_sources(SOURCES, |verse, entry| {
            let analysis = Analysis::default();
            let type_check_entry = |verse: &LspUniverse| {
                let mut world = verse.snapshot();
                world.set_is_compiling(false);
                let ctx = analysis.enter(WorldComputeGraph::from_world(world));
                let source = ctx.source_by_path(&entry).unwrap();
                ctx.shared_().type_check(&source)
            };

            type_check_entry(verse);
            let initial_count = query_count(&analysis, "type_check");
            assert_eq!(initial_count, 2);

            // An unchanged revision reuses the complete component histories.
            verse.increment_revision(|revision| revision.flush());
            type_check_entry(verse);
            assert_eq!(query_count(&analysis, "type_check"), initial_count);

            // A's lexical import interface is unchanged, but B's expression
            // input changed. A must not reuse a TypeInfo keyed only by A.
            let b_path = entry.parent().unwrap().join("b.typ");
            verse
                .map_shadow(&b_path, Bytes::from_string("#let value = \"x\""))
                .unwrap();
            type_check_entry(verse);
            assert_eq!(query_count(&analysis, "type_check"), initial_count + 2);
        });
    }

    #[test]
    fn docs() {
        snapshot_testing_at("..", "docs", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut docstrings = result.docstrings.iter().collect::<Vec<_>>();
            docstrings.sort_by(|x, y| x.0.cmp(y.0));
            let mut docstrings = docstrings
                .into_iter()
                .map(|(ident, expr)| {
                    format!(
                        "{} -> {expr:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                    )
                })
                .collect::<Vec<_>>();
            let mut snap = vec![];
            snap.push("= docstings".to_owned());
            snap.append(&mut docstrings);

            assert_snapshot!(snap.join("\n"));
        });
    }

    #[test]
    fn scope() {
        snapshot_testing_at("..", "expr_of", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.shared_().expr_stage(&source);
            let mut resolves = result.resolves.iter().collect::<Vec<_>>();
            resolves.sort_by(|x, y| x.1.decl.cmp(&y.1.decl));

            let mut resolves = resolves
                .into_iter()
                .map(|(_, expr)| {
                    let RefExpr {
                        decl: ident,
                        step,
                        root,
                        term,
                    } = expr.as_ref();

                    format!(
                        "{} -> {}, root {}, val: {term:?}",
                        source.show_expr(&Expr::Decl(ident.clone())),
                        step.as_ref()
                            .map(|expr| source.show_expr(expr))
                            .unwrap_or_default(),
                        root.as_ref()
                            .map(|expr| source.show_expr(expr))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>();
            let mut exports = result.exports.iter().collect::<Vec<_>>();
            exports.sort_by(|x, y| x.0.cmp(y.0));
            let mut exports = exports
                .into_iter()
                .map(|(ident, node)| {
                    let node = source.show_expr(node);
                    format!("{ident} -> {node}",)
                })
                .collect::<Vec<_>>();

            let mut snap = vec![];
            snap.push("= resolves".to_owned());
            snap.append(&mut resolves);
            snap.push("= exports".to_owned());
            snap.append(&mut exports);

            assert_snapshot!(snap.join("\n"));
        });
    }
}

#[cfg(test)]
mod module_tests {
    use serde_json::json;
    use tinymist_std::path::unix_slash;
    use typst::syntax::FileId;

    use crate::prelude::*;
    use crate::syntax::module::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing_at("..", "modules", &|ctx, _| {
            fn ids(ids: EcoVec<FileId>) -> Vec<String> {
                let mut ids: Vec<String> = ids
                    .into_iter()
                    .map(|id| unix_slash(id.vpath().as_rooted_path_compat()))
                    .collect();
                ids.sort();
                ids
            }

            let dependencies = construct_module_dependencies(ctx);

            let mut dependencies = dependencies
                .into_iter()
                .map(|(id, v)| {
                    (
                        unix_slash(id.vpath().as_rooted_path_compat()),
                        ids(v.dependencies),
                        ids(v.dependents),
                    )
                })
                .collect::<Vec<_>>();

            dependencies.sort();
            // remove /main.typ
            dependencies.retain(|(path, _, _)| path != "/main.typ");

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
mod type_check_tests {

    use core::fmt;

    use typst::syntax::Source;

    use crate::tests::*;
    use typst_shim::syntax::source_range;

    use crate::analysis::{Ty, TypeInfo};

    #[test]
    fn test() {
        snapshot_testing_at("..", "type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.type_check(&source);
            let result = format!("{:#?}", TypeCheckSnapshot(&source, &result));

            assert_snapshot!(result);
        });
    }

    struct TypeCheckSnapshot<'a>(&'a Source, &'a TypeInfo);

    impl fmt::Debug for TypeCheckSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let source = self.0;
            let info = self.1;
            let mut vars = info
                .vars
                .values()
                .map(|bounds| (bounds.name(), bounds))
                .collect::<Vec<_>>();

            vars.sort_by(|x, y| x.1.var.strict_cmp(&y.1.var));

            for (name, bounds) in vars {
                writeln!(f, "{name:?} = {:?}", info.simplify(bounds.as_type(), true))?;
            }

            writeln!(f, "=====")?;
            let mut mapping = info
                .mapping
                .iter()
                .map(|pair| (source_range(source, *pair.0).unwrap_or_default(), pair.1))
                .collect::<Vec<_>>();

            mapping.sort_by(|x, y| {
                x.0.start
                    .cmp(&y.0.start)
                    .then_with(|| x.0.end.cmp(&y.0.end))
            });

            for (range, value) in mapping {
                let ty = Ty::from_types(value.clone().into_iter());
                writeln!(f, "{range:?} -> {ty:?}")?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod post_type_check_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing_at("..", "post_type_check", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().full_text();

            let result = ctx.type_check(&source);
            let post_ty = post_type_check(ctx.shared_(), &result, node);

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let post_ty = post_ty.map(|ty| format!("{ty:#?}"))
                    .unwrap_or_else(|| "<nil>".to_string());
                assert_snapshot!(post_ty);
            })
        });
    }
}

#[cfg(test)]
mod type_describe_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing_at("..", "type_describe", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();
            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos + 1).unwrap();
            let text = node.get().clone().full_text();

            let ti = ctx.type_check(&source);
            let post_ty = post_type_check(ctx.shared_(), &ti, node);

            with_settings!({
                description => format!("Check on {text:?} ({pos:?})"),
            }, {
                let post_ty = post_ty.and_then(|ty| ty.describe())
                    .unwrap_or_else(|| "<nil>".into());
                assert_snapshot!(post_ty);
            })
        });
    }
}

#[cfg(test)]
mod signature_tests {

    use core::fmt;

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::{Signature, SignatureTarget, analyze_signature};
    use crate::syntax::classify_syntax;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing_at("..", "signature", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let callee_node = root.leaf_at_compat(pos).unwrap();
            let callee_node = classify_syntax(callee_node, pos).unwrap();
            let callee_node = callee_node.node();

            let result = analyze_signature(
                ctx.shared(),
                SignatureTarget::Syntax(source.clone(), callee_node.span()),
            );

            assert_snapshot!(SignatureSnapshot(result.as_ref()));
        });
    }

    struct SignatureSnapshot<'a>(pub Option<&'a Signature>);

    impl fmt::Display for SignatureSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(sig) = self.0 else {
                return write!(f, "<nil>");
            };

            let primary_sig = match sig {
                Signature::Primary(sig) => sig,
                Signature::Partial(sig) => {
                    for w in &sig.with_stack {
                        write!(f, "with ")?;
                        for arg in &w.items {
                            if let Some(name) = &arg.name {
                                write!(f, "{name}: ")?;
                            }
                            let term = arg.term.as_ref();
                            let term = term.and_then(|v| v.describe()).unwrap_or_default();
                            write!(f, "{term}, ")?;
                        }
                        f.write_str("\n")?;
                    }

                    &sig.signature
                }
            };

            writeln!(f, "fn(")?;
            for param in primary_sig.pos() {
                writeln!(f, " {},", param.name)?;
            }
            for param in primary_sig.named() {
                if let Some(expr) = &param.default {
                    writeln!(f, " {}: {},", param.name, expr)?;
                } else {
                    writeln!(f, " {},", param.name)?;
                }
            }
            if let Some(param) = primary_sig.rest() {
                writeln!(f, " ...{}, ", param.name)?;
            }
            write!(f, ")")?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod call_info_tests {

    use core::fmt;

    use typst::syntax::{LinkedNode, SyntaxKind};
    use typst_shim::syntax::LinkedNodeExt;

    use crate::analysis::analyze_call;
    use crate::tests::*;

    use crate::analysis::CallInfo;

    #[test]
    fn test() {
        snapshot_testing_at("..", "call_info", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let mut call_node = root.leaf_at_compat(pos + 1).unwrap();

            while let Some(parent) = call_node.parent() {
                if call_node.kind() == SyntaxKind::FuncCall {
                    break;
                }
                call_node = parent.clone();
            }

            let result = analyze_call(ctx, source.clone(), call_node);

            assert_snapshot!(CallSnapshot(result.as_deref()));
        });
    }

    struct CallSnapshot<'a>(pub Option<&'a CallInfo>);

    impl fmt::Display for CallSnapshot<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let Some(ci) = self.0 else {
                return write!(f, "<nil>");
            };

            let mut w = ci.arg_mapping.iter().collect::<Vec<_>>();
            w.sort_by(|x, y| x.0.span().into_raw().cmp(&y.0.span().into_raw()));

            for (arg, arg_call_info) in w {
                writeln!(f, "{} -> {:?}", arg.clone().full_text(), arg_call_info)?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod lint_tests {
    use std::collections::BTreeMap;

    use tinymist_lint::KnownIssues;

    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing_at("..", "lint", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let result = ctx.lint(&source, &KnownIssues::default());
            let result = crate::diagnostics::DiagWorker::new(ctx).convert_all(result.iter());
            let result = result
                .into_iter()
                .map(|(k, v)| (file_uri_(&k), v))
                .collect::<BTreeMap<_, _>>();
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
