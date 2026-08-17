//! Tests for the component coordinator and component-granular analysis.

use std::sync::{Arc, Barrier};

use rustc_hash::FxHashMap;
use typst::syntax::{RootedPath, VirtualPath, VirtualRoot};

use crate::analysis::TypeInfo;

use super::*;

fn file(name: &str) -> FileId {
    FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new(name).expect("test path must be valid"),
    ))
}

fn members(component: &AnalysisComponent) -> Vec<FileId> {
    component.members.iter().copied().collect()
}

fn discovery(dependencies: Vec<FileId>, has_unresolved: bool) -> DependencyDiscovery {
    DependencyDiscovery {
        dependencies,
        has_unresolved,
    }
}

#[test]
fn reverse_edge_merges_both_files_into_one_fresh_group() {
    // Allocate B first to ensure canonical order does not follow FileId
    // interning order.
    let b = file("component-fresh-b.typ");
    let a = file("component-fresh-a.typ");
    let coordinator = ComponentCoordinator::default();

    let component = coordinator.component_for(a, |fid| match fid {
        _ if fid == a => vec![b],
        _ if fid == b => vec![a],
        _ => vec![],
    });

    assert_eq!(members(&component), vec![a, b]);
    assert!(component.is_current());

    let state = coordinator.state.lock();
    assert_eq!(state.files[&a], state.files[&b]);
    // Eager relabeling removed both merged singleton groups.
    assert_eq!(state.groups.len(), 1);
}

#[test]
fn closing_a_three_node_cycle_merges_the_whole_cycle_region() {
    let a = file("component-cycle-a.typ");
    let b = file("component-cycle-b.typ");
    let c = file("component-cycle-c.typ");
    let coordinator = ComponentCoordinator::default();

    let component = coordinator.component_for(a, |fid| match fid {
        _ if fid == a => vec![b],
        _ if fid == b => vec![c],
        _ if fid == c => vec![a],
        _ => vec![],
    });

    assert_eq!(members(&component), vec![a, b, c]);
    let state = coordinator.state.lock();
    let group = state.group_for_file(a);
    assert_eq!(group, state.group_for_file(b));
    assert_eq!(group, state.group_for_file(c));
}

#[test]
fn acyclic_edges_keep_components_separate() {
    let a = file("component-dag-a.typ");
    let b = file("component-dag-b.typ");
    let c = file("component-dag-c.typ");
    let coordinator = ComponentCoordinator::default();

    let component_a = coordinator.component_for(a, |fid| match fid {
        _ if fid == a => vec![b],
        _ if fid == b => vec![c],
        _ => vec![],
    });
    let component_b = coordinator.component_for(b, |_| -> Vec<FileId> {
        panic!("an already discovered file must not be scanned again")
    });
    let component_c = coordinator.component_for(c, |_| -> Vec<FileId> {
        panic!("an already discovered file must not be scanned again")
    });

    assert_eq!(members(&component_a), vec![a]);
    assert_eq!(members(&component_b), vec![b]);
    assert_eq!(members(&component_c), vec![c]);
    assert!(!Arc::ptr_eq(&component_a, &component_b));
    assert!(!Arc::ptr_eq(&component_b, &component_c));
}

#[test]
fn late_acyclic_edge_keeps_existing_component_owners() {
    let a = file("component-late-dag-a.typ");
    let b = file("component-late-dag-b.typ");
    let coordinator = ComponentCoordinator::default();
    let component_a = coordinator.component_for(a, |_| vec![]);
    let component_b = coordinator.component_for(b, |_| vec![]);
    assert_eq!(coordinator.reachable_files(a), vec![a]);

    assert_eq!(
        coordinator.record_dependency(a, b),
        DependencyAdmission::Reachable
    );
    assert!(component_a.is_current());
    assert!(component_b.is_current());
    assert_eq!(coordinator.direct_dependencies(a), Some(vec![b]));
    assert_eq!(coordinator.reachable_files(a), vec![a, b]);
}

#[test]
fn late_cycle_redirects_sealed_components_to_a_fresh_owner() {
    let a = file("component-late-cycle-a.typ");
    let b = file("component-late-cycle-b.typ");
    let coordinator = ComponentCoordinator::default();
    let component_a = coordinator.component_for(a, |_| discovery(vec![], true));
    let component_b = coordinator.component_for(b, |fid| {
        assert_eq!(fid, b);
        vec![a]
    });

    assert_eq!(
        coordinator.record_dependency(a, b),
        DependencyAdmission::SameComponent
    );
    assert!(!component_a.is_current());
    assert!(!component_b.is_current());

    let merged = coordinator.component_for(a, |_| -> Vec<FileId> {
        panic!("all members of the late cycle were already discovered")
    });
    assert_eq!(members(&merged), vec![a, b]);
    assert!(merged.is_current());
    assert_eq!(
        coordinator.admit_dependency(a, b),
        DependencyAdmission::SameComponent
    );
}

#[test]
fn reachable_files_include_scc_and_condensation_closure_in_path_order() {
    let a = file("component-reachable-a.typ");
    let b = file("component-reachable-b.typ");
    let c = file("component-reachable-c.typ");
    let d = file("component-reachable-d.typ");
    let unknown = file("component-reachable-unknown.typ");
    let coordinator = ComponentCoordinator::default();

    coordinator.component_for(a, |fid| match fid {
        _ if fid == a => vec![b],
        _ if fid == b => vec![a, c],
        _ => vec![],
    });
    coordinator.component_for(d, |_| vec![]);

    assert_eq!(coordinator.reachable_files(a), vec![a, b, c]);
    assert_eq!(coordinator.reachable_files(b), vec![a, b, c]);
    assert_eq!(coordinator.reachable_files(c), vec![c]);
    assert_eq!(coordinator.reachable_files(d), vec![d]);
    assert!(coordinator.reachable_files(unknown).is_empty());
}

#[test]
fn unresolved_reachable_source_uses_no_wait_admission_without_mutating_graph() {
    let a = file("component-unresolved-a.typ");
    let b = file("component-unresolved-b.typ");
    let c = file("component-unresolved-c.typ");
    let unknown = file("component-unresolved-unknown.typ");
    let coordinator = ComponentCoordinator::default();

    let component_a = coordinator.component_for(a, |fid| match fid {
        _ if fid == a => discovery(vec![b], false),
        _ if fid == b => discovery(vec![], true),
        _ => discovery(vec![], false),
    });
    let component_b = coordinator.component_for(b, |_| -> DependencyDiscovery {
        panic!("an already discovered file must not be scanned again")
    });
    let component_c = coordinator.component_for(c, |_| discovery(vec![], false));

    assert!(!Arc::ptr_eq(&component_a, &component_b));
    assert!(!Arc::ptr_eq(&component_b, &component_c));
    assert!(coordinator.has_unresolved_dependencies(a));
    assert!(coordinator.has_unresolved_dependencies(b));
    assert!(!coordinator.has_unresolved_dependencies(c));
    assert!(coordinator.has_unresolved_dependencies(unknown));

    let before = {
        let state = coordinator.state.lock();
        (
            state.groups.len(),
            state.files.len(),
            state.dependencies.clone(),
            state.discovering.len(),
        )
    };

    assert_eq!(
        coordinator.admit_dependency(a, b),
        DependencyAdmission::Reachable
    );
    assert_eq!(
        coordinator.admit_dependency(b, b),
        DependencyAdmission::SameComponent
    );
    assert_eq!(
        coordinator.admit_dependency(a, c),
        DependencyAdmission::Unresolved
    );
    assert_eq!(
        coordinator.admit_dependency(b, a),
        DependencyAdmission::Unresolved
    );
    assert_eq!(
        coordinator.admit_dependency(c, a),
        DependencyAdmission::Rejected
    );

    let after = {
        let state = coordinator.state.lock();
        (
            state.groups.len(),
            state.files.len(),
            state.dependencies.clone(),
            state.discovering.len(),
        )
    };
    assert_eq!(before, after);
}

#[test]
fn dependency_discovery_is_exactly_once() {
    let a = file("component-once-a.typ");
    let b = file("component-once-b.typ");
    let coordinator = ComponentCoordinator::default();
    let mut counts = FxHashMap::<FileId, usize>::default();

    coordinator.component_for(a, |fid| {
        *counts.entry(fid).or_default() += 1;
        if fid == a { vec![b] } else { vec![] }
    });
    coordinator.component_for(a, |fid| {
        *counts.entry(fid).or_default() += 1;
        vec![]
    });
    coordinator.component_for(b, |fid| {
        *counts.entry(fid).or_default() += 1;
        vec![]
    });

    assert_eq!(counts.get(&a), Some(&1));
    assert_eq!(counts.get(&b), Some(&1));
}

#[test]
fn incomplete_graph_never_admits_a_wait_and_can_be_retried() {
    let a = file("component-retry-discovery-a.typ");
    let b = file("component-retry-discovery-b.typ");
    let coordinator = ComponentCoordinator::default();

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        coordinator.component_for(a, |fid| {
            if fid == a {
                vec![b]
            } else {
                panic!("test discovery panic")
            }
        });
    }));
    assert!(panicked.is_err());
    assert_eq!(
        coordinator.admit_dependency(a, b),
        DependencyAdmission::Unresolved
    );
    assert_eq!(
        coordinator.admit_dependency(b, b),
        DependencyAdmission::Unresolved
    );
    assert!(coordinator.has_unresolved_dependencies(a));
    assert!(coordinator.has_unresolved_dependencies(b));

    let component_a = coordinator.component_for(a, |fid| {
        assert_eq!(fid, b, "A was completed before the discovery panic");
        vec![]
    });
    let component_b = coordinator.component_for(b, |_| -> Vec<FileId> {
        panic!("the retry must complete B's discovery")
    });

    assert_eq!(members(&component_a), vec![a]);
    assert_eq!(members(&component_b), vec![b]);
    assert_eq!(
        coordinator.admit_dependency(a, b),
        DependencyAdmission::Reachable
    );
    assert_eq!(
        coordinator.admit_dependency(b, b),
        DependencyAdmission::SameComponent
    );
}

#[test]
fn independent_dependency_discovery_runs_in_parallel() {
    let a = file("component-discovery-parallel-a.typ");
    let b = file("component-discovery-parallel-b.typ");
    let coordinator = ComponentCoordinator::default();
    let barrier = Arc::new(Barrier::new(2));

    std::thread::scope(|scope| {
        for fid in [a, b] {
            let coordinator = coordinator.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                let component = coordinator.component_for(fid, |discovered| {
                    assert_eq!(discovered, fid);
                    barrier.wait();
                    Vec::<FileId>::new()
                });
                assert_eq!(members(&component), vec![fid]);
            });
        }
    });
}

#[test]
fn independent_component_result_slots_initialize_in_parallel() {
    let a = file("component-parallel-a.typ");
    let b = file("component-parallel-b.typ");
    let coordinator = ComponentCoordinator::default();
    let component_a = coordinator.component_for(a, |_| vec![]);
    let component_b = coordinator.component_for(b, |_| vec![]);
    let barrier = Arc::new(Barrier::new(2));

    std::thread::scope(|scope| {
        let barrier_a = barrier.clone();
        let component_a = component_a.clone();
        scope.spawn(move || {
            component_a.type_check.get_or_init(|| {
                barrier_a.wait();
                Arc::new(FxHashMap::from_iter([(a, Arc::new(TypeInfo::default()))]))
            });
        });

        let barrier_b = barrier.clone();
        let component_b = component_b.clone();
        scope.spawn(move || {
            component_b.type_check.get_or_init(|| {
                barrier_b.wait();
                Arc::new(FxHashMap::from_iter([(b, Arc::new(TypeInfo::default()))]))
            });
        });
    });

    assert!(component_a.type_check.get().is_some());
    assert!(component_b.type_check.get().is_some());
}

#[test]
fn panicked_batch_initializer_publishes_nothing_and_can_retry() {
    let a = file("component-retry-a.typ");
    let component = ComponentCoordinator::default().component_for(a, |_| vec![]);

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        component.expr_stage.get_or_init(|| panic!("test panic"));
    }));
    assert!(panicked.is_err());
    assert!(component.expr_stage.get().is_none());

    let result = component
        .expr_stage
        .get_or_init(|| Arc::new(FxHashMap::default()));
    assert!(result.is_empty());
}

mod expr_tests {

    use rayon::ThreadPoolBuilder;
    use tinymist_world::ShadowApi;
    use typst::foundations::Bytes;

    use crate::analysis::{Analysis, TypeInfo};
    use crate::syntax::Expr;
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
    fn cyclic_type_constraints_propagate_across_the_back_edge() {
        const SOURCES: &str = r#"
// path: a.typ
#import "b.typ": beta
#let alpha = 1
#let touch = beta
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

            let type_a = shared.type_check(&source_a);
            let type_b = shared.type_check(&source_b);
            let simplified = |info: &TypeInfo, name: &str| {
                let ty = info
                    .exports
                    .iter()
                    .find(|(export, _)| export.as_ref() == name)
                    .unwrap_or_else(|| panic!("missing exported type {name}"))
                    .1
                    .clone();
                format!("{:?}", info.simplify(ty, true))
            };

            let alpha = simplified(&type_a, "alpha");
            let beta = simplified(&type_b, "beta");
            assert_eq!(alpha, "1");
            assert_eq!(beta, "1");
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
}
