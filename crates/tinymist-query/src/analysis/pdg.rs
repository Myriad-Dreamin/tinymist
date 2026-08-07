//! Coordinates expression and type analysis at import-component granularity.

use std::{collections::VecDeque, sync::Arc};

use rustc_hash::FxHashSet;
use typst::syntax::FileId;

/// Discovers module dependencies and assigns files to sealed import SCCs.
///
/// A coordinator is revision-local. Clones share one graph mutex so that a
/// file is discovered exactly once and every directed-edge insertion, SCC
/// merge, and component seal is atomic with respect to other requests.
pub(crate) use crate::adt::pdg::{
    AnalysisComponent, ComponentCoordinator, DependencyAdmission, DependencyDiscovery,
};
use crate::adt::pdg::{ComponentId, CoordinatorState, Group, GroupState};

impl ComponentCoordinator {
    /// Returns the sealed component containing `root`.
    ///
    /// `discover` is called at most once for each file known to this
    /// coordinator. This method discovers the complete known forward closure
    /// before sealing any newly reached component, retaining unresolved sites
    /// as no-wait admission markers. The callback runs while the coordinator
    /// mutex is held and therefore must not recursively call this coordinator.
    pub(super) fn component_for<D>(
        &self,
        root: FileId,
        discover: impl FnMut(FileId) -> D,
    ) -> Arc<AnalysisComponent>
    where
        D: Into<DependencyDiscovery>,
    {
        self.state.lock().component_for(root, discover)
    }

    /// Determines whether analysis may acquire `target` from `source`.
    ///
    /// An expression import may select a module re-exported through an
    /// intermediate file. Such an import is safe when the admission graph
    /// already contains the equivalent transitive path. Otherwise, an
    /// unresolved dynamic dependency reachable from `source` requires a
    /// no-wait fallback. This method never mutates the frozen graph.
    pub(super) fn admit_dependency(&self, source: FileId, target: FileId) -> DependencyAdmission {
        self.state.lock().admit_dependency(source, target)
    }

    /// Whether `source` can reach an unresolved dynamic dependency site.
    ///
    /// An unknown source is treated conservatively as unresolved.
    pub(super) fn has_unresolved_dependencies(&self, source: FileId) -> bool {
        self.state.lock().has_unresolved_dependencies(source)
    }

    /// Returns every file reachable from `root` in the frozen admission graph.
    ///
    /// Members of `root`'s SCC are included. The result is ordered by rooted
    /// path so it is stable across file interning and worker schedules.
    pub(super) fn reachable_files(&self, root: FileId) -> Vec<FileId> {
        self.state.lock().reachable_files(root)
    }

    /// Returns the dependency identities captured when `source` was
    /// discovered, but only after its component graph has been sealed.
    pub(super) fn direct_dependencies(&self, source: FileId) -> Option<Vec<FileId>> {
        self.state.lock().direct_dependencies(source)
    }
}

impl CoordinatorState {
    fn component_for<D>(
        &mut self,
        root: FileId,
        mut discover: impl FnMut(FileId) -> D,
    ) -> Arc<AnalysisComponent>
    where
        D: Into<DependencyDiscovery>,
    {
        self.ensure_file(root);
        let root_group = self.group_for_file(root);
        if let GroupState::Sealed(component) = &self.groups[root_group].state {
            // Sealing freezes the complete known forward closure. A later
            // query for the same canonical group therefore has no graph work
            // to replay and can reuse its component owner immediately.
            return component.clone();
        }

        let mut pending = VecDeque::from([root]);
        let mut closure = FxHashSet::default();

        while let Some(fid) = pending.pop_front() {
            if !closure.insert(fid) {
                continue;
            }

            self.ensure_file(fid);
            let dependencies = if self.discovered.contains(&fid) {
                self.dependencies.get(&fid).cloned().unwrap_or_default()
            } else {
                let discovery = discover(fid).into();
                let mut dependencies = discovery.dependencies;
                dependencies.sort_unstable_by_key(|dep| dep.into_raw().get());
                dependencies.dedup();

                self.dependencies.insert(fid, dependencies.clone());
                self.discovered.insert(fid);

                for &dependency in &dependencies {
                    self.ensure_file(dependency);
                    self.add_edge(fid, dependency);
                }

                if discovery.has_unresolved {
                    let group = self.group_for_file(fid);
                    self.groups[group].has_unresolved_outgoing = true;
                }

                dependencies
            };

            pending.extend(dependencies);
        }

        let mut reached_groups = FxHashSet::default();
        for fid in closure {
            let group = self.group_for_file(fid);
            reached_groups.insert(group);
        }

        let mut reached_groups: Vec<_> = reached_groups.into_iter().collect();
        reached_groups.sort_unstable();
        for group in reached_groups {
            self.seal(group);
        }

        let root_group = self.group_for_file(root);
        match &self.groups[root_group].state {
            GroupState::Sealed(component) => component.clone(),
            GroupState::Open | GroupState::Redirect(_) => {
                unreachable!("the complete dependency closure must be sealed")
            }
        }
    }

    fn ensure_file(&mut self, fid: FileId) -> ComponentId {
        if let Some(&group) = self.files.get(&fid) {
            return self.find(group);
        }

        let id = self.groups.len();
        self.groups.push(Group {
            parent: id,
            members: vec![fid],
            has_unresolved_outgoing: false,
            state: GroupState::Open,
        });
        self.files.insert(fid, id);
        id
    }

    fn group_for_file(&mut self, fid: FileId) -> ComponentId {
        let group = self.files[&fid];
        self.find(group)
    }

    fn admit_dependency(&self, source: FileId, target: FileId) -> DependencyAdmission {
        let Some(&source) = self.files.get(&source) else {
            return DependencyAdmission::Unresolved;
        };
        let source = self.find_const(source);
        if !matches!(self.groups[source].state, GroupState::Sealed(_)) {
            // A discovery panic can leave a valid but incomplete Open graph.
            // Never turn that graph into a wait-for edge.
            return DependencyAdmission::Unresolved;
        }

        let target = self
            .files
            .get(&target)
            .map(|&target| self.find_const(target));
        if let Some(target) = target
            && source == target
        {
            return DependencyAdmission::SameComponent;
        }

        let reachable = self.reachable_set(source);
        if let Some(target) = target
            && reachable.contains(&target)
        {
            return DependencyAdmission::Reachable;
        }

        if reachable
            .into_iter()
            .any(|group| self.groups[group].has_unresolved_outgoing)
        {
            DependencyAdmission::Unresolved
        } else {
            DependencyAdmission::Rejected
        }
    }

    fn has_unresolved_dependencies(&self, source: FileId) -> bool {
        let Some(&source) = self.files.get(&source) else {
            return true;
        };
        let source = self.find_const(source);
        !matches!(self.groups[source].state, GroupState::Sealed(_))
            || self.group_has_unresolved_dependencies(source)
    }

    fn group_has_unresolved_dependencies(&self, source: ComponentId) -> bool {
        self.reachable_set(source)
            .into_iter()
            .any(|group| self.groups[group].has_unresolved_outgoing)
    }

    fn reachable_files(&self, root: FileId) -> Vec<FileId> {
        let Some(&root) = self.files.get(&root) else {
            return vec![];
        };
        let root = self.find_const(root);
        let mut files: Vec<_> = self
            .reachable_set(root)
            .into_iter()
            .flat_map(|group| self.groups[group].members.iter().copied())
            .collect();
        files.sort_by_cached_key(|fid| format!("{fid:?}"));
        files.dedup();
        files
    }

    fn direct_dependencies(&self, source: FileId) -> Option<Vec<FileId>> {
        let group = self.find_const(*self.files.get(&source)?);
        if !matches!(self.groups[group].state, GroupState::Sealed(_)) {
            return None;
        }
        self.dependencies.get(&source).cloned()
    }

    fn find(&mut self, mut group: ComponentId) -> ComponentId {
        let mut root = group;
        while self.groups[root].parent != root {
            root = self.groups[root].parent;
        }

        while self.groups[group].parent != group {
            let parent = self.groups[group].parent;
            debug_assert!(matches!(
                &self.groups[group].state,
                GroupState::Redirect(target) if *target == parent
            ));
            self.groups[group].parent = root;
            self.groups[group].state = GroupState::Redirect(root);
            group = parent;
        }

        root
    }

    fn find_const(&self, mut group: ComponentId) -> ComponentId {
        while self.groups[group].parent != group {
            group = self.groups[group].parent;
        }
        group
    }

    fn add_edge(&mut self, source: FileId, target: FileId) {
        let source = self.group_for_file(source);
        let target = self.group_for_file(target);
        if source == target {
            return;
        }

        assert!(
            matches!(&self.groups[source].state, GroupState::Open),
            "a sealed component cannot discover a new outgoing import"
        );

        let forward = self.reachable_set(target);
        if !forward.contains(&source) {
            return;
        }

        // The persistent graph is stored once in `dependencies`, indexed by
        // file. Build only the reverse index needed for this cycle search;
        // keeping incoming/outgoing sets on every historical Group would
        // duplicate and repeatedly rewrite the same edges during unions.
        let backward = self.reverse_reachable_set(source, &forward);
        let cycle: FxHashSet<_> = forward.intersection(&backward).copied().collect();

        debug_assert!(cycle.contains(&source));
        debug_assert!(cycle.contains(&target));
        self.merge(cycle);
    }

    fn reachable_set(&self, start: ComponentId) -> FxHashSet<ComponentId> {
        let start = self.find_const(start);
        let mut reached = FxHashSet::from_iter([start]);
        let mut pending = vec![start];

        while let Some(group) = pending.pop() {
            for &member in &self.groups[group].members {
                let Some(dependencies) = self.dependencies.get(&member) else {
                    continue;
                };
                for &dependency in dependencies {
                    let Some(&dependency_group) = self.files.get(&dependency) else {
                        continue;
                    };
                    let dependency_group = self.find_const(dependency_group);
                    if reached.insert(dependency_group) {
                        pending.push(dependency_group);
                    }
                }
            }
        }

        reached
    }

    fn reverse_reachable_set(
        &self,
        start: ComponentId,
        within: &FxHashSet<ComponentId>,
    ) -> FxHashSet<ComponentId> {
        let start = self.find_const(start);
        let mut reverse_edges = vec![Vec::new(); self.groups.len()];

        for (&source, dependencies) in &self.dependencies {
            let Some(&source_group) = self.files.get(&source) else {
                continue;
            };
            let source_group = self.find_const(source_group);
            if !within.contains(&source_group) {
                continue;
            }

            for &dependency in dependencies {
                let Some(&target_group) = self.files.get(&dependency) else {
                    continue;
                };
                let target_group = self.find_const(target_group);
                if within.contains(&target_group) && source_group != target_group {
                    reverse_edges[target_group].push(source_group);
                }
            }
        }

        let mut reached = FxHashSet::from_iter([start]);
        let mut pending = vec![start];
        while let Some(group) = pending.pop() {
            for &predecessor in &reverse_edges[group] {
                if reached.insert(predecessor) {
                    pending.push(predecessor);
                }
            }
        }

        reached
    }

    fn merge(&mut self, groups: FxHashSet<ComponentId>) -> ComponentId {
        let mut roots: Vec<_> = groups.into_iter().map(|group| self.find(group)).collect();
        roots.sort_unstable();
        roots.dedup();

        if roots.len() == 1 {
            return roots[0];
        }

        for &root in &roots {
            assert!(
                matches!(&self.groups[root].state, GroupState::Open),
                "a sealed component cannot be merged by a later import"
            );
        }

        let mut members = Vec::new();
        let mut has_unresolved_outgoing = false;

        for &root in &roots {
            members.extend(std::mem::take(&mut self.groups[root].members));
            has_unresolved_outgoing |= self.groups[root].has_unresolved_outgoing;
        }

        let fresh = self.groups.len();
        self.groups.push(Group {
            parent: fresh,
            members,
            has_unresolved_outgoing,
            state: GroupState::Open,
        });

        for root in roots {
            let group = &mut self.groups[root];
            group.parent = fresh;
            group.has_unresolved_outgoing = false;
            group.state = GroupState::Redirect(fresh);
        }

        fresh
    }

    fn seal(&mut self, group: ComponentId) -> Arc<AnalysisComponent> {
        let group = self.find(group);
        match &self.groups[group].state {
            GroupState::Sealed(component) => return component.clone(),
            GroupState::Redirect(_) => unreachable!("canonical groups cannot be redirects"),
            GroupState::Open => {}
        }

        assert!(
            self.groups[group]
                .members
                .iter()
                .all(|member| self.discovered.contains(member)),
            "a component cannot be sealed before every member is discovered"
        );

        let members = self.groups[group].members.to_vec();
        let component = Arc::new(AnalysisComponent::new(members));
        self.groups[group].state = GroupState::Sealed(component.clone());
        component
    }
}

#[cfg(test)]
mod tests {
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
    fn reverse_edge_redirects_both_roots_to_a_fresh_group() {
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

        let mut state = coordinator.state.lock();
        let old_a = state.files[&a];
        let old_b = state.files[&b];
        let fresh = state.find(old_a);
        assert_eq!(fresh, state.find(old_b));
        assert_ne!(fresh, old_a);
        assert_ne!(fresh, old_b);
        assert!(matches!(
            &state.groups[old_a].state,
            GroupState::Redirect(target) if *target == fresh
        ));
        assert!(matches!(
            &state.groups[old_b].state,
            GroupState::Redirect(target) if *target == fresh
        ));
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
        let mut state = coordinator.state.lock();
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
                state
                    .groups
                    .iter()
                    .map(|group| {
                        (
                            group.parent,
                            group.members.clone(),
                            group.has_unresolved_outgoing,
                        )
                    })
                    .collect::<Vec<_>>(),
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
                state
                    .groups
                    .iter()
                    .map(|group| {
                        (
                            group.parent,
                            group.members.clone(),
                            group.has_unresolved_outgoing,
                        )
                    })
                    .collect::<Vec<_>>(),
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
}
#[cfg(test)]
mod expr_tests {

    use rayon::ThreadPoolBuilder;
    use tinymist_world::ShadowApi;
    use typst::foundations::Bytes;

    use crate::analysis::Analysis;
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
