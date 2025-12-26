//! Generic worklist dataflow solver for [`Cfg`](super::cfg::Cfg).

use std::collections::VecDeque;

use super::cfg::{Cfg, NodeId};

#[derive(Debug, Clone)]
/// In/out states computed for each CFG node.
pub struct DataflowSolution<S> {
    /// State before applying the transfer function at each node.
    pub in_states: Vec<S>,
    /// State after applying the transfer function at each node.
    pub out_states: Vec<S>,
}

/// A forward dataflow problem.
///
/// This solver assumes the analysis state forms a **join-semilattice**:
/// - `bottom()` returns the **least element** `⊥` (neutral element of `join`):
///   `join(⊥, x) = x` and `join(x, ⊥) = x`.
/// - `join` must be **commutative**, **associative**, and **idempotent**.
/// - `transfer` must be **monotone** with respect to the partial order induced
///   by `join` (so the worklist iteration reaches a fixed point).
///
/// Note: whether `⊥` feels "most conservative" or "most optimistic" depends on
/// how you define your analysis order; the only requirement here is that `⊥` is
/// the least element for the `join` you provide.
///
/// `initial_in/initial_out` are per-node boundary conditions / seeds. The solver
/// does **not** treat `entry`/`exit` specially; if you need a classic
/// single-entry boundary condition, return a non-`bottom` value only for the
/// entry node.
pub trait ForwardDataflowProblem<N> {
    /// The lattice element computed per CFG node.
    type State: Clone + PartialEq;

    /// The least element `⊥` of the join-semilattice.
    fn bottom(&self) -> Self::State;

    /// Optional seed for the node's `in` state (boundary condition).
    fn initial_in(&self, _node: NodeId, _node_data: &N) -> Self::State {
        self.bottom()
    }

    /// Optional seed for the node's `out` state (boundary condition).
    fn initial_out(&self, _node: NodeId, _node_data: &N) -> Self::State {
        self.bottom()
    }

    /// Joins two lattice elements.
    fn join(&self, left: &Self::State, right: &Self::State) -> Self::State;
    /// Applies the node transfer function.
    fn transfer(&self, node: NodeId, node_data: &N, in_state: &Self::State) -> Self::State;
}

/// A backward dataflow problem.
///
/// See [`ForwardDataflowProblem`] for the required lattice/monotonicity
/// guarantees. `initial_out` plays the same role as "exit-state" in classic
/// formulations, but is expressed as a per-node boundary condition to keep the
/// solver independent from CFG notions like "the" exit node.
pub trait BackwardDataflowProblem<N> {
    /// The lattice element computed per CFG node.
    type State: Clone + PartialEq;

    /// The least element `⊥` of the join-semilattice.
    fn bottom(&self) -> Self::State;

    /// Optional seed for the node's `in` state (boundary condition).
    fn initial_in(&self, _node: NodeId, _node_data: &N) -> Self::State {
        self.bottom()
    }

    /// Optional seed for the node's `out` state (boundary condition).
    fn initial_out(&self, _node: NodeId, _node_data: &N) -> Self::State {
        self.bottom()
    }

    /// Joins two lattice elements.
    fn join(&self, left: &Self::State, right: &Self::State) -> Self::State;
    /// Applies the node transfer function.
    fn transfer(&self, node: NodeId, node_data: &N, out_state: &Self::State) -> Self::State;
}

/// Solves a forward dataflow problem on the given CFG.
pub fn solve_forward<N, E, P>(cfg: &Cfg<N, E>, problem: &P) -> DataflowSolution<P::State>
where
    P: ForwardDataflowProblem<N>,
{
    let len = cfg.len();
    let mut in_states = vec![problem.bottom(); len];
    let mut out_states = vec![problem.bottom(); len];

    let mut in_init = vec![problem.bottom(); len];
    let mut out_init = vec![problem.bottom(); len];
    for (idx, node_data) in cfg.nodes().iter().enumerate() {
        let id = NodeId::from(idx);
        in_init[idx] = problem.initial_in(id, node_data);
        out_init[idx] = problem.initial_out(id, node_data);
        in_states[idx] = in_init[idx].clone();
        out_states[idx] = out_init[idx].clone();
    }

    let mut q = VecDeque::new();
    let mut in_worklist = vec![false; len];
    for (idx, in_wl) in in_worklist.iter_mut().enumerate() {
        q.push_back(NodeId::from(idx));
        *in_wl = true;
    }

    while let Some(n) = q.pop_front() {
        in_worklist[n.index()] = false;
        let node_data = cfg.node(n);

        let mut it = cfg.predecessors(n).iter().copied();
        let mut acc = match it.next() {
            Some(first) => out_states[first.index()].clone(),
            None => problem.bottom(),
        };
        for p in it {
            acc = problem.join(&acc, &out_states[p.index()]);
        }
        let new_in = problem.join(&in_init[n.index()], &acc);

        if new_in != in_states[n.index()] {
            in_states[n.index()] = new_in;
        }

        let transferred = problem.transfer(n, node_data, &in_states[n.index()]);
        let new_out = problem.join(&out_init[n.index()], &transferred);
        if new_out == out_states[n.index()] {
            continue;
        }
        out_states[n.index()] = new_out;

        for e in cfg.successors(n) {
            let s = e.to;
            if !in_worklist[s.index()] {
                q.push_back(s);
                in_worklist[s.index()] = true;
            }
        }
    }

    DataflowSolution {
        in_states,
        out_states,
    }
}

/// Solves a backward dataflow problem on the given CFG.
pub fn solve_backward<N, E, P>(cfg: &Cfg<N, E>, problem: &P) -> DataflowSolution<P::State>
where
    P: BackwardDataflowProblem<N>,
{
    let len = cfg.len();
    let mut in_states = vec![problem.bottom(); len];
    let mut out_states = vec![problem.bottom(); len];

    let mut in_init = vec![problem.bottom(); len];
    let mut out_init = vec![problem.bottom(); len];
    for (idx, node_data) in cfg.nodes().iter().enumerate() {
        let id = NodeId::from(idx);
        in_init[idx] = problem.initial_in(id, node_data);
        out_init[idx] = problem.initial_out(id, node_data);
        in_states[idx] = in_init[idx].clone();
        out_states[idx] = out_init[idx].clone();
    }

    let mut q = VecDeque::new();
    let mut in_worklist = vec![false; len];
    for (idx, in_wl) in in_worklist.iter_mut().enumerate() {
        q.push_back(NodeId::from(idx));
        *in_wl = true;
    }

    while let Some(n) = q.pop_front() {
        in_worklist[n.index()] = false;
        let node_data = cfg.node(n);

        let mut it = cfg.successors(n).iter();
        let joined = match it.next() {
            Some(first) => {
                let mut acc = in_states[first.to.index()].clone();
                for s in it {
                    acc = problem.join(&acc, &in_states[s.to.index()]);
                }
                acc
            }
            None => problem.bottom(),
        };
        let new_out = problem.join(&out_init[n.index()], &joined);

        if new_out != out_states[n.index()] {
            out_states[n.index()] = new_out;
        }

        let transferred = problem.transfer(n, node_data, &out_states[n.index()]);
        let new_in = problem.join(&in_init[n.index()], &transferred);
        if new_in == in_states[n.index()] {
            continue;
        }
        in_states[n.index()] = new_in;

        for &p in cfg.predecessors(n) {
            if !in_worklist[p.index()] {
                q.push_back(p);
                in_worklist[p.index()] = true;
            }
        }
    }

    DataflowSolution {
        in_states,
        out_states,
    }
}
