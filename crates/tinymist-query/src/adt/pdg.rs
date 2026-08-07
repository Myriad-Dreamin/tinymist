//! Data types owned by the program dependency graph.

use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::FileId;

use crate::{analysis::TypeInfo, syntax::ExprInfo};

pub(crate) type ComponentId = usize;

/// Dependencies admitted before a component starts analysis.
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
    /// The target is reachable through the frozen condensation DAG.
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
///
/// Components are sealed only after the complete known forward dependency
/// closure has been discovered. Unresolved dynamic sites remain explicit and
/// cannot acquire an unadmitted result slot. Consequently, component-level
/// result slots are never invalidated by a later merge in the same revision.
pub(crate) struct AnalysisComponent {
    /// Members of the component in stable file-id order.
    pub(crate) members: Arc<[FileId]>,
    /// Complete expression results for all component members.
    pub(crate) expr_stage: OnceLock<Arc<FxHashMap<FileId, ExprInfo>>>,
    /// Complete type-check results for all component members.
    pub(crate) type_check: OnceLock<Arc<FxHashMap<FileId, Arc<TypeInfo>>>>,
}

impl AnalysisComponent {
    pub(crate) fn new(mut members: Vec<FileId>) -> Self {
        // FileId allocation order can depend on which request touched a path
        // first. Sort by the stable rooted path so the component owner always
        // starts from the same file across worker schedules and revisions.
        members.sort_by_cached_key(|fid| format!("{fid:?}"));
        members.dedup();

        Self {
            members: members.into(),
            expr_stage: OnceLock::new(),
            type_check: OnceLock::new(),
        }
    }
}

/// Revision-local coordinator for the program dependency graph.
#[derive(Clone, Default)]
pub(crate) struct ComponentCoordinator {
    pub(crate) state: Arc<Mutex<CoordinatorState>>,
}

/// Mutable graph state protected by [`ComponentCoordinator::state`].
#[derive(Default)]
pub(crate) struct CoordinatorState {
    pub(crate) files: FxHashMap<FileId, ComponentId>,
    pub(crate) dependencies: FxHashMap<FileId, Vec<FileId>>,
    pub(crate) discovered: FxHashSet<FileId>,
    pub(crate) groups: Vec<Group>,
}

/// One incremental SCC group in the graph.
pub(crate) struct Group {
    pub(crate) parent: ComponentId,
    pub(crate) members: Vec<FileId>,
    pub(crate) has_unresolved_outgoing: bool,
    pub(crate) state: GroupState,
}

/// Lifecycle of an incremental SCC group.
pub(crate) enum GroupState {
    /// The group can still receive discovered edges.
    Open,
    /// The old root has been merged into the fresh group identified here.
    Redirect(ComponentId),
    /// The group has a stable component owner and cannot be merged again.
    Sealed(Arc<AnalysisComponent>),
}
