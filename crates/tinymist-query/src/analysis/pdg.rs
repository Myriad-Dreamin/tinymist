//! Coordinates expression and type analysis at import-component granularity.

use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
};

use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};
use typst::{
    foundations::Value,
    syntax::{FileId, SyntaxKind, SyntaxNode, ast, ast::AstNode},
};

use super::{SharedContext, TypeInfo};
use crate::{
    adt::union_find::{SetId, UnionFind},
    syntax::{ExprInfo, resolve_id_by_path},
};

type ComponentId = SetId;

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
    /// Union-find root that owns this result batch.
    id: ComponentId,
    /// Members of the component in stable file-id order.
    pub(crate) members: Arc<[FileId]>,
    /// Complete expression results for all component members.
    pub(crate) expr_stage: OnceLock<Arc<FxHashMap<FileId, ExprInfo>>>,
    /// Complete type-check results for all component members.
    pub(crate) type_check: OnceLock<Arc<FxHashMap<FileId, Arc<TypeInfo>>>>,
}

impl AnalysisComponent {
    fn new(id: ComponentId, mut members: Vec<FileId>) -> Self {
        // FileId allocation order can depend on which request touched a path
        // first. Sort by the stable rooted path so every worker chooses the
        // same component member order.
        members.sort_by_cached_key(|fid| format!("{fid:?}"));
        assert!(
            members.windows(2).all(|pair| pair[0] != pair[1]),
            "a union-find component must contain every file exactly once"
        );

        Self {
            id,
            members: members.into(),
            expr_stage: OnceLock::new(),
            type_check: OnceLock::new(),
        }
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
    files: FxHashMap<FileId, ComponentId>,
    dependencies: FxHashMap<FileId, Vec<FileId>>,
    discovering: FxHashMap<FileId, Arc<OnceLock<DependencyDiscovery>>>,
    groups: UnionFind<Group>,
    reachability: FxHashMap<ComponentId, Reachability>,
}

#[derive(Clone)]
struct Reachability {
    groups: Arc<[ComponentId]>,
    files: Arc<[FileId]>,
    has_unresolved: bool,
}

/// Analysis state attached to a canonical union-find root.
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
    /// both the edge insertion and any union caused by the new edge.
    pub(super) fn record_dependency(&self, source: FileId, target: FileId) -> DependencyAdmission {
        self.state.lock().record_dependency(source, target)
    }

    /// Whether `component` still owns its union-find root.
    pub(super) fn is_current(&self, component: &Arc<AnalysisComponent>) -> bool {
        self.state.lock().is_current(component)
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

    /// Returns the dependency identities captured when `source` was
    /// discovered, but only after its component graph has been sealed.
    pub(super) fn direct_dependencies(&self, source: FileId) -> Option<Vec<FileId>> {
        self.state.lock().direct_dependencies(source)
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
    fn sealed_component(&mut self, fid: FileId) -> Option<Arc<AnalysisComponent>> {
        let group = self.group_for_file(fid);
        match self.groups.value(group) {
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
            match self.groups.value_mut(group) {
                Group::Open {
                    has_unresolved_outgoing,
                    ..
                } => *has_unresolved_outgoing = true,
                Group::Sealed { .. } => {
                    unreachable!("discovery cannot update a sealed component")
                }
            }
        }

        dependencies
    }

    fn ensure_file(&mut self, fid: FileId) -> ComponentId {
        if let Some(&group) = self.files.get(&fid) {
            return self.groups.find_mut(group);
        }

        let id = self.groups.insert(Group::Open {
            members: vec![fid],
            has_unresolved_outgoing: false,
        });
        self.files.insert(fid, id);
        id
    }

    fn group_for_file(&mut self, fid: FileId) -> ComponentId {
        let group = self.files[&fid];
        self.groups.find_mut(group)
    }

    fn is_current(&self, component: &Arc<AnalysisComponent>) -> bool {
        if self.groups.find(component.id) != component.id {
            return false;
        }
        matches!(
            self.groups.value(component.id),
            Group::Sealed { component: current, .. } if Arc::ptr_eq(current, component)
        )
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
        let source = self.groups.find(source);
        if !matches!(self.groups.value(source), Group::Sealed { .. }) {
            // A discovery panic can leave a valid but incomplete Open graph.
            // Never turn that graph into a wait-for edge.
            return DependencyAdmission::Unresolved;
        }

        let target = self
            .files
            .get(&target)
            .map(|&target| self.groups.find(target));
        if let Some(target) = target
            && source == target
        {
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
        let source = self.groups.find(source);
        !matches!(self.groups.value(source), Group::Sealed { .. })
            || self.group_has_unresolved_dependencies(source)
    }

    fn group_has_unresolved_dependencies(&mut self, source: ComponentId) -> bool {
        self.reachability(source).has_unresolved
    }

    fn reachable_files(&mut self, root: FileId) -> Vec<FileId> {
        let Some(&root) = self.files.get(&root) else {
            return vec![];
        };
        let root = self.groups.find(root);
        self.reachability(root).files.to_vec()
    }

    fn reachability(&mut self, start: ComponentId) -> Reachability {
        let start = self.groups.find_mut(start);
        if let Some(cached) = self.reachability.get(&start) {
            return cached.clone();
        }

        let mut groups: Vec<_> = self.reachable_set(start).into_iter().collect();
        groups.sort_unstable();
        let has_unresolved = groups
            .iter()
            .any(|&group| self.groups.value(group).has_unresolved_outgoing());
        let mut files: Vec<_> = groups
            .iter()
            .flat_map(|&group| self.groups.value(group).members().iter().copied())
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

    fn direct_dependencies(&self, source: FileId) -> Option<Vec<FileId>> {
        let group = self.groups.find(*self.files.get(&source)?);
        if !matches!(self.groups.value(group), Group::Sealed { .. }) {
            return None;
        }
        self.dependencies.get(&source).cloned()
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
        let start = self.groups.find(start);
        let mut reached = FxHashSet::from_iter([start]);
        let mut pending = vec![start];

        while let Some(group) = pending.pop() {
            for &member in self.groups.value(group).members() {
                let Some(dependencies) = self.dependencies.get(&member) else {
                    continue;
                };
                for &dependency in dependencies {
                    let Some(&dependency_group) = self.files.get(&dependency) else {
                        continue;
                    };
                    let dependency_group = self.groups.find(dependency_group);
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
        let start = self.groups.find(start);
        let mut reverse_edges = FxHashMap::<ComponentId, Vec<ComponentId>>::default();

        for (&source, dependencies) in &self.dependencies {
            let Some(&source_group) = self.files.get(&source) else {
                continue;
            };
            let source_group = self.groups.find(source_group);
            if !within.contains(&source_group) {
                continue;
            }

            for &dependency in dependencies {
                let Some(&target_group) = self.files.get(&dependency) else {
                    continue;
                };
                let target_group = self.groups.find(target_group);
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

    fn merge(&mut self, groups: FxHashSet<ComponentId>) -> ComponentId {
        let mut roots: Vec<_> = groups
            .into_iter()
            .map(|group| self.groups.find_mut(group))
            .collect();
        roots.sort_unstable();
        roots.dedup();

        if roots.len() == 1 {
            return roots[0];
        }

        self.reachability.clear();
        self.groups.merge_into_new(roots, |groups| {
            let mut members = Vec::new();
            let mut has_unresolved_outgoing = false;
            for group in groups {
                match group {
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
                        members.extend(component.members.iter().copied());
                        has_unresolved_outgoing |= group_has_unresolved;
                    }
                };
            }

            Group::Open {
                members,
                has_unresolved_outgoing,
            }
        })
    }

    fn seal(&mut self, group: ComponentId) -> Arc<AnalysisComponent> {
        let group = self.groups.find_mut(group);
        if let Group::Sealed { component, .. } = self.groups.value(group) {
            return component.clone();
        }

        assert!(
            self.groups
                .value(group)
                .members()
                .iter()
                .all(|member| self.dependencies.contains_key(member)),
            "a component cannot be sealed before every member is discovered"
        );

        let members = self.groups.value(group).members().to_vec();
        let has_unresolved_outgoing = self.groups.value(group).has_unresolved_outgoing();
        let component = Arc::new(AnalysisComponent::new(group, members));
        *self.groups.value_mut(group) = Group::Sealed {
            component: component.clone(),
            has_unresolved_outgoing,
        };
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
        let fresh = state.groups.find_mut(old_a);
        assert_eq!(fresh, state.groups.find_mut(old_b));
        assert_ne!(fresh, old_a);
        assert_ne!(fresh, old_b);
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
        assert!(coordinator.is_current(&component_a));
        assert!(coordinator.is_current(&component_b));
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
        assert!(!coordinator.is_current(&component_a));
        assert!(!coordinator.is_current(&component_b));

        let merged = coordinator.component_for(a, |_| -> Vec<FileId> {
            panic!("all members of the late cycle were already discovered")
        });
        assert_eq!(members(&merged), vec![a, b]);
        assert!(coordinator.is_current(&merged));
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
}
#[cfg(test)]
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
