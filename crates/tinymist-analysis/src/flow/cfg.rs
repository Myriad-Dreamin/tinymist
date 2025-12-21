//! Minimal CFG graph representation used by analyses.
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A stable index into a [`Cfg`].
pub struct NodeId(usize);

impl NodeId {
    /// Returns the underlying node index.
    #[inline]
    pub fn index(self) -> usize {
        self.0
    }
}

impl From<usize> for NodeId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// A simple directed control-flow graph with explicit entry/exit nodes.
#[derive(Debug, Clone)]
pub struct Cfg<N> {
    nodes: Vec<N>,
    succ: Vec<Vec<NodeId>>,
    pred: Vec<Vec<NodeId>>,
    /// Entry node for reachability queries and boundary conditions.
    pub entry: NodeId,
    /// Exit node for reachability queries and boundary conditions.
    pub exit: NodeId,
}

impl<N> Cfg<N> {
    /// Creates a new CFG with the given entry/exit node payloads.
    pub fn new(entry: N, exit: N) -> Self {
        Self {
            nodes: vec![entry, exit],
            succ: vec![Vec::new(), Vec::new()],
            pred: vec![Vec::new(), Vec::new()],
            entry: NodeId(0),
            exit: NodeId(1),
        }
    }

    #[inline]
    /// Returns the number of nodes in this CFG.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    /// Returns whether this CFG has zero nodes.
    ///
    /// This is typically `false` because CFGs always have `entry` and `exit`.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[inline]
    /// Returns all node payloads in allocation order.
    pub fn nodes(&self) -> &[N] {
        &self.nodes
    }

    #[inline]
    /// Returns the payload for the given node.
    pub fn node(&self, id: NodeId) -> &N {
        &self.nodes[id.index()]
    }

    #[inline]
    /// Returns the successors of the given node.
    pub fn successors(&self, id: NodeId) -> &[NodeId] {
        &self.succ[id.index()]
    }

    #[inline]
    /// Returns the predecessors of the given node.
    pub fn predecessors(&self, id: NodeId) -> &[NodeId] {
        &self.pred[id.index()]
    }

    /// Adds a new node and returns its [`NodeId`].
    pub fn add_node(&mut self, node: N) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        self.succ.push(Vec::new());
        self.pred.push(Vec::new());
        id
    }

    /// Adds a directed edge `from -> to`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        debug_assert!(
            !self.succ[from.index()].contains(&to),
            "duplicate edge: {from:?} -> {to:?}"
        );
        debug_assert!(
            !self.pred[to.index()].contains(&from),
            "duplicate edge (pred): {from:?} -> {to:?}"
        );
        self.succ[from.index()].push(to);
        self.pred[to.index()].push(from);
    }

    /// Returns a boolean vector marking nodes reachable from `start`.
    pub fn reachable_from(&self, start: NodeId) -> Vec<bool> {
        let mut reachable = vec![false; self.nodes.len()];
        let mut q = VecDeque::new();
        reachable[start.index()] = true;
        q.push_back(start);

        while let Some(n) = q.pop_front() {
            for &m in &self.succ[n.index()] {
                if !reachable[m.index()] {
                    reachable[m.index()] = true;
                    q.push_back(m);
                }
            }
        }

        reachable
    }

    #[inline]
    /// Returns a boolean vector marking nodes reachable from `entry`.
    pub fn reachable_from_entry(&self) -> Vec<bool> {
        self.reachable_from(self.entry)
    }
}
