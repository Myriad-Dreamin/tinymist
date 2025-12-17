use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::Span;

use super::ir::*;

/// Returns blocks that are structurally unreachable because the builder had no
/// incoming edges for them (typically code after `return`/`break`/`continue`).
pub fn orphan_blocks(cfg: &ControlFlowGraph) -> Vec<BlockId> {
    let preds = cfg.predecessors();
    (0..cfg.blocks.len())
        .map(BlockId)
        .filter(|&bb| {
            bb != cfg.entry && bb != cfg.exit && bb != cfg.error_exit && preds[bb.0].is_empty()
        })
        .collect()
}

/// Returns a best-effort mapping from statement spans to blocks.
pub fn stmt_index(cfg: &ControlFlowGraph) -> FxHashMap<Span, BlockId> {
    let mut map = FxHashMap::default();
    for (bb_idx, bb) in cfg.blocks.iter().enumerate() {
        let bb_id = BlockId(bb_idx);
        for stmt in &bb.stmts {
            map.entry(stmt.span).or_insert(bb_id);
        }
    }
    map
}

/// Dominator tree information for a CFG.
#[derive(Debug, Clone)]
pub struct Dominators {
    /// Immediate dominator for each block (or `None` if unreachable).
    pub idom: Vec<Option<BlockId>>,
    /// Reverse postorder of reachable blocks.
    pub rpo: Vec<BlockId>,
}

impl Dominators {
    /// Returns whether block `a` dominates block `b`.
    pub fn dominates(&self, a: BlockId, mut b: BlockId) -> bool {
        if a == b {
            return true;
        }
        while let Some(idom) = self.idom.get(b.0).and_then(|v| *v) {
            if idom == a {
                return true;
            }
            if idom == b {
                break;
            }
            b = idom;
        }
        false
    }
}

/// Computes dominators for `cfg` (restricted to reachable blocks).
pub fn dominators(cfg: &ControlFlowGraph) -> Dominators {
    let preds = cfg.predecessors();
    let reachable = cfg.reachable_blocks();

    // Reverse postorder numbering.
    fn dfs(
        cfg: &ControlFlowGraph,
        reachable: &FxHashSet<BlockId>,
        bb: BlockId,
        seen: &mut FxHashSet<BlockId>,
        post: &mut Vec<BlockId>,
    ) {
        if !reachable.contains(&bb) || !seen.insert(bb) {
            return;
        }
        for succ in cfg.successors(bb).into_iter().flatten() {
            dfs(cfg, reachable, succ, seen, post);
        }
        post.push(bb);
    }

    let mut post = Vec::new();
    dfs(
        cfg,
        &reachable,
        cfg.entry,
        &mut FxHashSet::default(),
        &mut post,
    );
    let mut rpo = post;
    rpo.reverse();

    let mut rpo_index: Vec<Option<usize>> = vec![None; cfg.blocks.len()];
    for (i, bb) in rpo.iter().enumerate() {
        rpo_index[bb.0] = Some(i);
    }

    let mut idom: Vec<Option<BlockId>> = vec![None; cfg.blocks.len()];
    idom[cfg.entry.0] = Some(cfg.entry);

    let intersect = |idom: &Vec<Option<BlockId>>,
                     rpo_index: &Vec<Option<usize>>,
                     mut f1: BlockId,
                     mut f2: BlockId|
     -> BlockId {
        while f1 != f2 {
            while rpo_index[f1.0].unwrap_or(usize::MAX) > rpo_index[f2.0].unwrap_or(usize::MAX) {
                f1 = idom[f1.0].unwrap();
            }
            while rpo_index[f2.0].unwrap_or(usize::MAX) > rpo_index[f1.0].unwrap_or(usize::MAX) {
                f2 = idom[f2.0].unwrap();
            }
        }
        f1
    };

    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new_idom: Option<BlockId> = None;
            for &p in &preds[b.0] {
                if !reachable.contains(&p) {
                    continue;
                }
                if idom[p.0].is_none() {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => p,
                    Some(q) => intersect(&idom, &rpo_index, p, q),
                });
            }
            if idom[b.0] != new_idom {
                idom[b.0] = new_idom;
                changed = true;
            }
        }
    }

    Dominators { idom, rpo }
}

/// Returns all back edges `(from, to)` where `to` dominates `from`.
pub fn back_edges(cfg: &ControlFlowGraph, dom: &Dominators) -> Vec<(BlockId, BlockId)> {
    let mut edges = Vec::new();
    for from in 0..cfg.blocks.len() {
        let from = BlockId(from);
        for to in cfg.successors(from).into_iter().flatten() {
            if dom.dominates(to, from) {
                edges.push((from, to));
            }
        }
    }
    edges
}

/// Computes the natural loop induced by a back edge `back -> header`.
pub fn natural_loop(cfg: &ControlFlowGraph, header: BlockId, back: BlockId) -> FxHashSet<BlockId> {
    let preds = cfg.predecessors();
    let mut set: FxHashSet<BlockId> = FxHashSet::default();
    set.insert(header);
    set.insert(back);
    let mut stack = vec![back];
    while let Some(n) = stack.pop() {
        for &p in &preds[n.0] {
            if set.insert(p) {
                stack.push(p);
            }
        }
    }
    set
}
