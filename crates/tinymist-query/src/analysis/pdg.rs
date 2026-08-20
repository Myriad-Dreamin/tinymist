//! Coordinates expression and type analysis at import-component granularity.

use std::{
    collections::VecDeque,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};
use typst::{
    foundations::Value,
    syntax::{FileId, SyntaxKind, SyntaxNode, ast, ast::AstNode},
};

use super::{SharedContext, TypeInfo};
use crate::syntax::{ExprInfo, resolve_id_by_path};

/// Identifies one live group in [`CoordinatorState::groups`].
///
/// Identities are never reused. A merge removes the merged groups from the
/// group map and relabels their files to a fresh identity, so a stale
/// identity is simply absent instead of being redirected.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ComponentId(usize);

/// Dependencies found before a component starts analysis.
#[derive(Clone)]
pub(crate) struct DependencyDiscovery {
    /// Dependencies whose targets were resolved during discovery.
    pub(crate) dependencies: Vec<FileId>,
    /// Whether at least one dependency site could not be resolved completely.
    pub(crate) has_unresolved: bool,
}

impl From<Vec<FileId>> for DependencyDiscovery {
    fn from(dependencies: Vec<FileId>) -> Self {
        Self {
            dependencies,
            has_unresolved: false,
        }
    }
}

/// Whether a dependency may acquire another component's completed result.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum DependencyAdmission {
    /// Both files are members of the same SCC and use the same local route.
    SameComponent,
    /// The target is reachable through the current condensation DAG.
    Reachable,
    /// Discovery retained an unresolved dynamic edge on a reachable source.
    ///
    /// The caller must use an unknown/unconstrained fallback without acquiring
    /// the target component's result slot.
    Unresolved,
    /// Discovery proved neither a path nor a possible unresolved path.
    Rejected,
}

/// A sealed strongly connected component of the module import graph.
pub(crate) struct AnalysisComponent {
    /// Members of the component in stable file-id order.
    pub(crate) members: Arc<[FileId]>,
    /// Set when a late dynamic edge merges this batch into a fresh owner.
    retracted: AtomicBool,
    /// Complete expression results for all component members.
    pub(crate) expr_stage: OnceLock<Arc<FxHashMap<FileId, ExprInfo>>>,
    /// Complete type-check results for all component members.
    pub(crate) type_check: OnceLock<Arc<FxHashMap<FileId, Arc<TypeInfo>>>>,
}

impl AnalysisComponent {
    fn new(mut members: Vec<FileId>) -> Self {
        // FileId allocation order can depend on which request touched a path
        // first. Sort by the stable rooted path so every worker chooses the
        // same component member order.
        members.sort_by_cached_key(|fid| format!("{fid:?}"));
        assert!(
            members.windows(2).all(|pair| pair[0] != pair[1]),
            "a sealed component must contain every file exactly once"
        );

        Self {
            members: members.into(),
            retracted: AtomicBool::new(false),
            expr_stage: OnceLock::new(),
            type_check: OnceLock::new(),
        }
    }

    /// Whether this batch still owns its component.
    ///
    /// This flag is advisory: a merge can retract the owner the instant after
    /// it is read. Retry loops use it to discard obsolete batches cheaply.
    /// Anything whose correctness depends on ownership — reading dependency
    /// identities or publishing a batch to revision history — must instead go
    /// through [`ComponentCoordinator::commit_current`], which verifies
    /// ownership and runs the action in one critical section.
    pub(super) fn is_current(&self) -> bool {
        !self.retracted.load(Ordering::Acquire)
    }

    fn retract(&self) {
        self.retracted.store(true, Ordering::Release);
    }
}

/// Revision-local coordinator for the program dependency graph.
#[derive(Clone, Default)]
pub(crate) struct ComponentCoordinator {
    state: Arc<Mutex<CoordinatorState>>,
}

/// Mutable graph state protected by [`ComponentCoordinator::state`].
#[derive(Default)]
struct CoordinatorState {
    /// The current group of every known file. Merges relabel these entries
    /// eagerly, so a stored identity is always live.
    files: FxHashMap<FileId, ComponentId>,
    dependencies: FxHashMap<FileId, Vec<FileId>>,
    discovering: FxHashMap<FileId, Arc<OnceLock<DependencyDiscovery>>>,
    groups: FxHashMap<ComponentId, Group>,
    next_group: usize,
    reachability: FxHashMap<ComponentId, Reachability>,
}

#[derive(Clone)]
struct Reachability {
    groups: Arc<[ComponentId]>,
    files: Arc<[FileId]>,
    has_unresolved: bool,
}

/// Analysis state attached to a live group.
enum Group {
    Open {
        members: Vec<FileId>,
        has_unresolved_outgoing: bool,
    },
    Sealed {
        component: Arc<AnalysisComponent>,
        has_unresolved_outgoing: bool,
    },
}

impl Group {
    fn members(&self) -> &[FileId] {
        match self {
            Self::Open { members, .. } => members,
            Self::Sealed { component, .. } => &component.members,
        }
    }

    fn has_unresolved_outgoing(&self) -> bool {
        match self {
            Self::Open {
                has_unresolved_outgoing,
                ..
            } => *has_unresolved_outgoing,
            Self::Sealed {
                has_unresolved_outgoing,
                ..
            } => *has_unresolved_outgoing,
        }
    }
}

impl ComponentCoordinator {
    /// Returns the sealed component containing `root`.
    ///
    /// `discover` is called at most once for each file known to this
    /// coordinator. This method discovers the complete known forward closure
    /// before sealing any newly reached component, retaining unresolved sites
    /// as no-wait admission markers. Slow callbacks run outside the graph
    /// mutex; concurrent callers only wait when they request the same file.
    pub(super) fn component_for<D>(
        &self,
        root: FileId,
        mut discover: impl FnMut(FileId) -> D,
    ) -> Arc<AnalysisComponent>
    where
        D: Into<DependencyDiscovery>,
    {
        {
            let mut state = self.state.lock();
            state.ensure_file(root);
            if let Some(component) = state.sealed_component(root) {
                return component;
            }
        }

        let mut pending = VecDeque::from([root]);
        let mut closure = FxHashSet::default();

        while let Some(fid) = pending.pop_front() {
            if !closure.insert(fid) {
                continue;
            }

            let claim = {
                let mut state = self.state.lock();
                state.ensure_file(fid);
                if let Some(dependencies) = state.dependencies.get(&fid) {
                    Ok(dependencies.clone())
                } else {
                    Err(state
                        .discovering
                        .entry(fid)
                        .or_insert_with(|| Arc::new(OnceLock::new()))
                        .clone())
                }
            };

            let dependencies = match claim {
                Ok(dependencies) => dependencies,
                Err(slot) => {
                    // OnceLock retains no value when initialization panics, so
                    // the same discovery claim remains safely retryable.
                    let discovery = slot.get_or_init(|| discover(fid).into()).clone();
                    self.state.lock().commit_discovery(fid, discovery)
                }
            };
            pending.extend(dependencies);
        }

        let mut state = self.state.lock();
        let mut reached_groups = FxHashSet::default();
        for fid in closure {
            reached_groups.insert(state.group_for_file(fid));
        }

        let mut reached_groups: Vec<_> = reached_groups.into_iter().collect();
        reached_groups.sort_unstable();
        for group in reached_groups {
            state.seal(group);
        }

        state
            .sealed_component(root)
            .expect("the complete dependency closure must be sealed")
    }

    /// Determines whether analysis may acquire `target` from `source`.
    ///
    /// An expression import may select a module re-exported through an
    /// intermediate file. Such an import is safe when the admission graph
    /// already contains the equivalent transitive path. Otherwise, an
    /// unresolved dynamic dependency reachable from `source` requires a
    /// no-wait fallback. This read-only query never inserts a new edge.
    pub(super) fn admit_dependency(&self, source: FileId, target: FileId) -> DependencyAdmission {
        self.state.lock().admit_dependency(source, target)
    }

    /// Records a dependency resolved by the full expression pass.
    ///
    /// This is the dynamic counterpart to discovery. The graph mutex protects
    /// both the edge insertion and any merge caused by the new edge.
    pub(super) fn record_dependency(&self, source: FileId, target: FileId) -> DependencyAdmission {
        self.state.lock().record_dependency(source, target)
    }

    /// Whether `source` can reach an unresolved dynamic dependency site.
    ///
    /// An unknown source is treated conservatively as unresolved.
    pub(super) fn has_unresolved_dependencies(&self, source: FileId) -> bool {
        self.state.lock().has_unresolved_dependencies(source)
    }

    /// Returns every file reachable from `root` in the current admission graph.
    ///
    /// Members of `root`'s SCC are included. The result is ordered by rooted
    /// path so it is stable across file interning and worker schedules.
    pub(super) fn reachable_files(&self, root: FileId) -> Vec<FileId> {
        self.state.lock().reachable_files(root)
    }

    /// Runs `commit` iff `component` still owns its sealed group, in one
    /// critical section on the graph mutex.
    ///
    /// Ownership is verified by pointer identity against the live group, and
    /// `commit` receives every member's discovery-time dependency identities.
    /// Because [`CoordinatorState::merge`] retracts owners under the same
    /// mutex, no merge can interleave between the ownership check, the
    /// dependency reads, and whatever `commit` publishes. Returns `None`
    /// without calling `commit` when the component no longer owns its group.
    pub(super) fn commit_current<R>(
        &self,
        component: &AnalysisComponent,
        commit: impl FnOnce(&FxHashMap<FileId, Vec<FileId>>) -> R,
    ) -> Option<R> {
        let state = self.state.lock();
        if !state.owns_group(component) {
            return None;
        }
        Some(commit(&state.member_dependencies(component)))
    }

    /// Returns one consistent snapshot of every member's discovery-time
    /// dependency identities, or `None` when `component` was retracted.
    pub(super) fn member_dependencies(
        &self,
        component: &AnalysisComponent,
    ) -> Option<FxHashMap<FileId, Vec<FileId>>> {
        let state = self.state.lock();
        state
            .owns_group(component)
            .then(|| state.member_dependencies(component))
    }
}

impl SharedContext {
    pub(super) fn analysis_component(self: &Arc<Self>, fid: FileId) -> Arc<AnalysisComponent> {
        self.components()
            .component_for(fid, |fid| self.discover_dependencies(fid))
    }

    fn discover_dependencies(self: &Arc<Self>, fid: FileId) -> DependencyDiscovery {
        fn add_value(
            ctx: &SharedContext,
            fid: FileId,
            value: Value,
            dependencies: &mut FxHashSet<FileId>,
        ) -> bool {
            let target = match value {
                Value::Str(path) => resolve_id_by_path(ctx.world(), fid, path.as_str()),
                Value::Module(module) => module.file_id(),
                _ => None,
            };
            let Some(target) = target else {
                return false;
            };
            dependencies.insert(target);
            true
        }

        let Ok(source) = self.source_by_id(fid) else {
            return DependencyDiscovery {
                dependencies: vec![],
                has_unresolved: true,
            };
        };
        let mut dependencies = FxHashSet::default();
        let mut has_unresolved = false;
        Self::walk_dependency_sites(source.root(), &mut |kind, site| {
            crate::log_debug_ct!(
                "dependency discovery: {fid:?} {kind:?} at {:?}",
                site.span()
            );

            // Reuse the normal const/type-backed import evaluator. It first
            // handles direct constants and then falls back to the existing
            // runtime import tracer; this pass does not maintain a second
            // lexical evaluator.
            let (source_value, module_value) = self.analyze_import(site);
            let mut resolved = false;
            for value in source_value.into_iter().chain(module_value) {
                resolved |= add_value(self, fid, value, &mut dependencies);
            }

            // A traced result is only an observation for a non-literal site.
            // Keep dynamic sites eligible for an authoritative late edge from
            // the full expression pass.
            let exact_literal = site
                .cast::<ast::Expr>()
                .is_some_and(|expr| matches!(expr, ast::Expr::Str(_)));
            has_unresolved |= !exact_literal || !resolved;
        });
        let mut dependencies: Vec<_> = dependencies.into_iter().collect();
        dependencies.sort_unstable_by_key(|dependency| dependency.into_raw().get());
        DependencyDiscovery {
            dependencies,
            has_unresolved,
        }
    }

    fn walk_dependency_sites(node: &SyntaxNode, f: &mut impl FnMut(SyntaxKind, &SyntaxNode)) {
        match node.kind() {
            SyntaxKind::ModuleImport => {
                if let Some(import) = node.cast::<ast::ModuleImport>() {
                    f(SyntaxKind::ModuleImport, import.source().to_untyped());
                }
            }
            SyntaxKind::ModuleInclude => {
                if let Some(include) = node.cast::<ast::ModuleInclude>() {
                    f(SyntaxKind::ModuleInclude, include.source().to_untyped());
                }
            }
            _ => {}
        }

        for child in node.children() {
            Self::walk_dependency_sites(child, f);
        }
    }

    pub(super) fn dependency_admission(
        &self,
        source: FileId,
        target: FileId,
    ) -> DependencyAdmission {
        self.components().admit_dependency(source, target)
    }
}

impl CoordinatorState {
    fn next_id(&mut self) -> ComponentId {
        let id = ComponentId(self.next_group);
        self.next_group += 1;
        id
    }

    fn group_for_file(&self, fid: FileId) -> ComponentId {
        self.files[&fid]
    }

    fn sealed_component(&self, fid: FileId) -> Option<Arc<AnalysisComponent>> {
        match &self.groups[&self.group_for_file(fid)] {
            Group::Sealed { component, .. } => Some(component.clone()),
            Group::Open { .. } => None,
        }
    }

    fn commit_discovery(&mut self, fid: FileId, mut discovery: DependencyDiscovery) -> Vec<FileId> {
        if let Some(dependencies) = self.dependencies.get(&fid) {
            return dependencies.clone();
        }

        discovery
            .dependencies
            .sort_unstable_by_key(|dep| dep.into_raw().get());
        discovery.dependencies.dedup();
        let dependencies = discovery.dependencies;
        self.dependencies.insert(fid, dependencies.clone());
        self.discovering.remove(&fid);
        self.reachability.clear();

        for &dependency in &dependencies {
            self.ensure_file(dependency);
            self.add_edge(fid, dependency);
        }

        if discovery.has_unresolved {
            let group = self.group_for_file(fid);
            match self.groups.get_mut(&group) {
                Some(Group::Open {
                    has_unresolved_outgoing,
                    ..
                }) => *has_unresolved_outgoing = true,
                _ => unreachable!("discovery cannot update a sealed or dead component"),
            }
        }

        dependencies
    }

    fn ensure_file(&mut self, fid: FileId) -> ComponentId {
        if let Some(&group) = self.files.get(&fid) {
            return group;
        }

        let id = self.next_id();
        self.groups.insert(
            id,
            Group::Open {
                members: vec![fid],
                has_unresolved_outgoing: false,
            },
        );
        self.files.insert(fid, id);
        id
    }

    fn record_dependency(&mut self, source: FileId, target: FileId) -> DependencyAdmission {
        if !self.dependencies.contains_key(&source) {
            return DependencyAdmission::Unresolved;
        }

        self.ensure_file(target);
        let dependencies = self
            .dependencies
            .get_mut(&source)
            .expect("the source dependency record was checked above");
        if !dependencies.contains(&target) {
            dependencies.push(target);
            dependencies.sort_unstable_by_key(|dep| dep.into_raw().get());
            self.reachability.clear();
            self.add_edge(source, target);
        }

        if self.group_for_file(source) == self.group_for_file(target) {
            DependencyAdmission::SameComponent
        } else {
            // The just-recorded direct edge proves reachability even when the
            // target's own discovery is still in progress.
            DependencyAdmission::Reachable
        }
    }

    fn admit_dependency(&mut self, source: FileId, target: FileId) -> DependencyAdmission {
        let Some(&source) = self.files.get(&source) else {
            return DependencyAdmission::Unresolved;
        };
        if !matches!(self.groups[&source], Group::Sealed { .. }) {
            // A discovery panic can leave a valid but incomplete Open graph.
            // Never turn that graph into a wait-for edge.
            return DependencyAdmission::Unresolved;
        }

        let target = self.files.get(&target).copied();
        if target == Some(source) {
            return DependencyAdmission::SameComponent;
        }

        let reachable = self.reachability(source);
        if let Some(target) = target
            && reachable.groups.contains(&target)
        {
            return DependencyAdmission::Reachable;
        }

        if reachable.has_unresolved {
            DependencyAdmission::Unresolved
        } else {
            DependencyAdmission::Rejected
        }
    }

    fn has_unresolved_dependencies(&mut self, source: FileId) -> bool {
        let Some(&source) = self.files.get(&source) else {
            return true;
        };
        !matches!(self.groups[&source], Group::Sealed { .. })
            || self.reachability(source).has_unresolved
    }

    fn reachable_files(&mut self, root: FileId) -> Vec<FileId> {
        let Some(&root) = self.files.get(&root) else {
            return vec![];
        };
        self.reachability(root).files.to_vec()
    }

    fn reachability(&mut self, start: ComponentId) -> Reachability {
        if let Some(cached) = self.reachability.get(&start) {
            return cached.clone();
        }

        let mut groups: Vec<_> = self.reachable_set(start).into_iter().collect();
        groups.sort_unstable();
        let has_unresolved = groups
            .iter()
            .any(|&group| self.groups[&group].has_unresolved_outgoing());
        let mut files: Vec<_> = groups
            .iter()
            .flat_map(|&group| self.groups[&group].members().iter().copied())
            .collect();
        files.sort_by_cached_key(|fid| format!("{fid:?}"));
        assert!(
            files.windows(2).all(|pair| pair[0] != pair[1]),
            "condensation reachability must contain each file once"
        );

        let result = Reachability {
            groups: groups.into(),
            files: files.into(),
            has_unresolved,
        };
        self.reachability.insert(start, result.clone());
        result
    }

    /// Whether `component` is still the sealed owner of its members' group,
    /// verified by pointer identity against the live group map.
    fn owns_group(&self, component: &AnalysisComponent) -> bool {
        let Some(&group) = self.files.get(&component.members[0]) else {
            return false;
        };
        matches!(
            &self.groups[&group],
            Group::Sealed { component: current, .. }
                if std::ptr::eq(Arc::as_ptr(current), component)
        )
    }

    /// Snapshots every member's discovery-time dependency identities.
    ///
    /// Only meaningful while the caller has verified ownership under the same
    /// lock guard; sealing guarantees every member has been discovered.
    fn member_dependencies(&self, component: &AnalysisComponent) -> FxHashMap<FileId, Vec<FileId>> {
        component
            .members
            .iter()
            .map(|&fid| {
                let dependencies = self
                    .dependencies
                    .get(&fid)
                    .expect("a sealed member must have committed dependencies");
                (fid, dependencies.clone())
            })
            .collect()
    }

    fn add_edge(&mut self, source: FileId, target: FileId) {
        let source = self.group_for_file(source);
        let target = self.group_for_file(target);
        if source == target {
            return;
        }

        let forward = self.reachable_set(target);
        if !forward.contains(&source) {
            return;
        }

        // The new edge closes at least one cycle. Its region is every group
        // that the edge target reaches and that reaches the edge source back.
        // The reverse index is built once per closing edge and restricted to
        // `forward`, so a long chain closing a cycle stays O(V + E) instead
        // of degrading to one forward search per candidate group.
        let backward = self.reverse_reachable_set(source, &forward);
        let cycle: FxHashSet<_> = forward.intersection(&backward).copied().collect();

        debug_assert!(cycle.contains(&source));
        debug_assert!(cycle.contains(&target));
        self.merge(cycle);
    }

    fn reverse_reachable_set(
        &self,
        start: ComponentId,
        within: &FxHashSet<ComponentId>,
    ) -> FxHashSet<ComponentId> {
        let mut reverse_edges = FxHashMap::<ComponentId, Vec<ComponentId>>::default();

        for (&source, dependencies) in &self.dependencies {
            let Some(&source_group) = self.files.get(&source) else {
                continue;
            };
            if !within.contains(&source_group) {
                continue;
            }

            for &dependency in dependencies {
                let Some(&target_group) = self.files.get(&dependency) else {
                    continue;
                };
                if within.contains(&target_group) && source_group != target_group {
                    reverse_edges
                        .entry(target_group)
                        .or_default()
                        .push(source_group);
                }
            }
        }

        let mut reached = FxHashSet::from_iter([start]);
        let mut pending = vec![start];
        while let Some(group) = pending.pop() {
            for &predecessor in reverse_edges.get(&group).into_iter().flatten() {
                if reached.insert(predecessor) {
                    pending.push(predecessor);
                }
            }
        }

        reached
    }

    fn reachable_set(&self, start: ComponentId) -> FxHashSet<ComponentId> {
        let mut reached = FxHashSet::from_iter([start]);
        let mut pending = vec![start];

        while let Some(group) = pending.pop() {
            for &member in self.groups[&group].members() {
                let Some(dependencies) = self.dependencies.get(&member) else {
                    continue;
                };
                for &dependency in dependencies {
                    let Some(&dependency_group) = self.files.get(&dependency) else {
                        continue;
                    };
                    if reached.insert(dependency_group) {
                        pending.push(dependency_group);
                    }
                }
            }
        }

        reached
    }

    /// Replaces every group in `groups` by one fresh open group.
    ///
    /// Sealed constituents are retracted here, in the same critical section
    /// that relabels their members, so [`AnalysisComponent::is_current`]
    /// reflects the ownership change atomically.
    fn merge(&mut self, groups: FxHashSet<ComponentId>) -> ComponentId {
        let mut roots: Vec<_> = groups.into_iter().collect();
        roots.sort_unstable();
        if let [only] = roots[..] {
            return only;
        }

        self.reachability.clear();
        let mut members = Vec::new();
        let mut has_unresolved_outgoing = false;
        for group in roots {
            match self
                .groups
                .remove(&group)
                .expect("a merged group must be live")
            {
                Group::Open {
                    members: group_members,
                    has_unresolved_outgoing: group_has_unresolved,
                } => {
                    members.extend(group_members);
                    has_unresolved_outgoing |= group_has_unresolved;
                }
                Group::Sealed {
                    component,
                    has_unresolved_outgoing: group_has_unresolved,
                } => {
                    component.retract();
                    members.extend(component.members.iter().copied());
                    has_unresolved_outgoing |= group_has_unresolved;
                }
            }
        }

        let fresh = self.next_id();
        for &member in &members {
            self.files.insert(member, fresh);
        }
        self.groups.insert(
            fresh,
            Group::Open {
                members,
                has_unresolved_outgoing,
            },
        );
        fresh
    }

    fn seal(&mut self, group: ComponentId) -> Arc<AnalysisComponent> {
        if let Group::Sealed { component, .. } = &self.groups[&group] {
            return component.clone();
        }

        assert!(
            self.groups[&group]
                .members()
                .iter()
                .all(|member| self.dependencies.contains_key(member)),
            "a component cannot be sealed before every member is discovered"
        );

        let members = self.groups[&group].members().to_vec();
        let has_unresolved_outgoing = self.groups[&group].has_unresolved_outgoing();
        let component = Arc::new(AnalysisComponent::new(members));
        self.groups.insert(
            group,
            Group::Sealed {
                component: component.clone(),
                has_unresolved_outgoing,
            },
        );
        component
    }
}

#[cfg(test)]
mod tests {
    use typst::syntax::{RootedPath, VirtualPath, VirtualRoot};

    use super::*;

    fn file(name: &str) -> FileId {
        FileId::new(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new(name).expect("test path must be valid"),
        ))
    }

    #[test]
    fn commit_current_is_linearized_with_late_cycle_merges() {
        let a = file("commit-late-cycle-a.typ");
        let b = file("commit-late-cycle-b.typ");
        let coordinator = ComponentCoordinator::default();
        let component_a = coordinator.component_for(a, |_| DependencyDiscovery {
            dependencies: vec![],
            has_unresolved: true,
        });
        let component_b = coordinator.component_for(b, |fid| {
            assert_eq!(fid, b);
            vec![a]
        });

        let deps_a = coordinator
            .commit_current(&component_a, |deps| deps.clone())
            .expect("the sealed owner must be committable");
        assert_eq!(deps_a.get(&a), Some(&vec![]));

        // The late back edge merges both sealed owners into a fresh group; a
        // commit racing with the merge must observe the retraction.
        assert_eq!(
            coordinator.record_dependency(a, b),
            DependencyAdmission::SameComponent
        );
        assert!(coordinator.commit_current(&component_a, |_| ()).is_none());
        assert!(coordinator.commit_current(&component_b, |_| ()).is_none());
        assert!(coordinator.member_dependencies(&component_a).is_none());

        let merged = coordinator.component_for(a, |_| -> Vec<FileId> {
            panic!("all members of the late cycle were already discovered")
        });
        let deps = coordinator
            .member_dependencies(&merged)
            .expect("the fresh owner must provide a dependency snapshot");
        assert_eq!(deps.get(&a), Some(&vec![b]));
        assert_eq!(deps.get(&b), Some(&vec![a]));
        assert!(coordinator.commit_current(&merged, |_| ()).is_some());
    }
}
