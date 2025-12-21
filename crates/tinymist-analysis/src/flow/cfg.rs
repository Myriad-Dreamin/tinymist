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

#[derive(Debug, Clone)]
/// A directed edge in a [`Cfg`].
pub struct Edge<E> {
    /// Successor node id.
    pub to: NodeId,
    /// Edge payload.
    pub data: E,
}

/// A simple directed control-flow graph with explicit entry/exit nodes.
#[derive(Debug, Clone)]
pub struct Cfg<N, E = ()> {
    nodes: Vec<N>,
    succ: Vec<Vec<Edge<E>>>,
    pred: Vec<Vec<NodeId>>,
    /// Entry node for reachability queries and boundary conditions.
    pub entry: NodeId,
    /// Exit node for reachability queries and boundary conditions.
    pub exit: NodeId,
}

impl<N> Cfg<N, ()> {
    /// Adds a directed edge `from -> to`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        debug_assert!(
            !self.succ[from.index()].iter().any(|e| e.to == to),
            "duplicate edge: {from:?} -> {to:?}"
        );
        debug_assert!(
            !self.pred[to.index()].contains(&from),
            "duplicate edge (pred): {from:?} -> {to:?}"
        );
        self.succ[from.index()].push(Edge { to, data: () });
        self.pred[to.index()].push(from);
    }
}

impl<N, E> Cfg<N, E> {
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
    pub fn successors(&self, id: NodeId) -> &[Edge<E>] {
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

    /// Adds a directed edge `from -> to` with an edge payload.
    ///
    /// Note: This does not enforce uniqueness of `(from, to)` pairs because
    /// labeled CFGs may legitimately have multiple edges to the same successor.
    pub fn add_edge_with(&mut self, from: NodeId, to: NodeId, data: E) {
        self.succ[from.index()].push(Edge { to, data });
        self.pred[to.index()].push(from);
    }

    /// Returns a boolean vector marking nodes reachable from `start`.
    pub fn reachable_from(&self, start: NodeId) -> Vec<bool> {
        let mut reachable = vec![false; self.nodes.len()];
        let mut q = VecDeque::new();
        reachable[start.index()] = true;
        q.push_back(start);

        while let Some(n) = q.pop_front() {
            for e in &self.succ[n.index()] {
                if !reachable[e.to.index()] {
                    reachable[e.to.index()] = true;
                    q.push_back(e.to);
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
