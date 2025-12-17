use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::{Span, SyntaxKind};

/// Identifier of a CFG "body" within a [`CfgCollection`].
///
/// A "body" corresponds to an executable region: the file root or a nested
/// closure body.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(pub usize);

/// Identifier of a basic block within a [`ControlFlowGraph`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub usize);

/// Kind of a CFG body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKind {
    /// The file/root markup body.
    Root,
    /// A nested closure body.
    Closure,
}

/// A collection of CFG bodies built from syntax trees.
#[derive(Debug, Clone)]
pub struct CfgCollection {
    /// All built bodies.
    pub bodies: Vec<ControlFlowGraph>,
    /// Mapping from closure expression spans to their body ids.
    pub closure_bodies: FxHashMap<Span, BodyId>,
    /// Mapping from declaration spans (e.g. `let f = (..) => ..`) to their body ids.
    pub decl_bodies: FxHashMap<Span, BodyId>,
}

impl CfgCollection {
    /// Returns the CFG for `id`.
    pub fn body(&self, id: BodyId) -> &ControlFlowGraph {
        &self.bodies[id.0]
    }

    /// Returns the root body id, if any.
    pub fn root(&self) -> Option<BodyId> {
        (!self.bodies.is_empty()).then_some(BodyId(0))
    }

    /// Returns the body id for a closure expression span.
    pub fn closure_body(&self, closure_span: Span) -> Option<BodyId> {
        self.closure_bodies.get(&closure_span).copied()
    }

    /// Returns the body id for a declaration span.
    pub fn decl_body(&self, decl_span: Span) -> Option<BodyId> {
        self.decl_bodies.get(&decl_span).copied()
    }
}

/// A control-flow graph for a single body (root or closure).
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Body id within the owning [`CfgCollection`].
    pub id: BodyId,
    /// Body kind (root or closure).
    pub kind: BodyKind,
    /// Span of the source region that produced this body.
    pub origin: Span,

    /// Entry basic block.
    pub entry: BlockId,
    /// Normal exit block.
    pub exit: BlockId,
    /// Error exit block for illegal control flow.
    pub error_exit: BlockId,

    /// All basic blocks in this body.
    pub blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    /// Returns a block by id.
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }

    /// Returns up to two successor blocks of `id`.
    pub fn successors(&self, id: BlockId) -> [Option<BlockId>; 2] {
        self.block(id).terminator.successors()
    }

    /// Computes predecessor lists for all blocks.
    pub fn predecessors(&self) -> Vec<Vec<BlockId>> {
        let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); self.blocks.len()];
        for from in 0..self.blocks.len() {
            let from = BlockId(from);
            for succ in self.successors(from).into_iter().flatten() {
                preds[succ.0].push(from);
            }
        }
        preds
    }

    /// Computes the set of blocks reachable from [`ControlFlowGraph::entry`].
    pub fn reachable_blocks(&self) -> FxHashSet<BlockId> {
        let mut seen: FxHashSet<BlockId> = FxHashSet::default();
        let mut stack = vec![self.entry];
        while let Some(bb) = stack.pop() {
            if !seen.insert(bb) {
                continue;
            }
            for succ in self.successors(bb).into_iter().flatten() {
                stack.push(succ);
            }
        }
        seen
    }

    /// Basic debug dump that stays stable enough for snapshot tests.
    pub fn debug_dump(&self) -> String {
        use core::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            &mut out,
            "Body {:?} origin={:?} entry={:?} exit={:?} error_exit={:?}",
            self.kind, self.origin, self.entry, self.exit, self.error_exit
        );
        for (i, bb) in self.blocks.iter().enumerate() {
            let _ = writeln!(
                &mut out,
                "  bb{:#?}: stmts={} term={:?}",
                BlockId(i),
                bb.stmts.len(),
                bb.terminator
            );
        }
        out
    }
}

/// A basic block: a sequence of statements ending in a [`Terminator`].
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Statement-like items recorded for diagnostics.
    pub stmts: Vec<Stmt>,
    /// Terminator that defines outgoing edges.
    pub terminator: Terminator,
}

/// A statement-like item recorded in a block.
#[derive(Debug, Clone)]
pub struct Stmt {
    /// Span of the originating syntax node.
    pub span: Span,
    /// Syntax kind of the originating node.
    pub kind: SyntaxKind,
}

/// Kind of CFG exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitKind {
    /// Normal completion.
    Normal,
    /// Error completion.
    Error,
}

/// Kind of conditional edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchKind {
    /// `if` / `else`.
    If,
    /// `while` condition.
    While,
    /// `for` iteration step.
    ForIter,
    /// Short-circuit `and`.
    And,
    /// Short-circuit `or`.
    Or,
}

/// Terminator of a basic block.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Temporary placeholder during construction.
    Unset,
    /// Exit the current body.
    Exit(ExitKind),
    /// Unconditional jump.
    Goto(BlockId),
    /// Conditional branch (including short-circuit edges).
    Branch {
        /// Branch type.
        kind: BranchKind,
        /// Span of the condition/operator.
        span: Span,
        /// Successor for the "then"/true edge.
        then_bb: BlockId,
        /// Successor for the "else"/false edge.
        else_bb: BlockId,
    },
    /// `return` from a closure/context boundary.
    Return {
        /// Span of the `return`.
        span: Span,
        /// Target block (normal exit if allowed, error exit otherwise).
        target: BlockId,
        /// Whether this `return` is syntactically allowed here.
        allowed: bool,
    },
    /// `break` from a loop.
    Break {
        /// Span of the `break`.
        span: Span,
        /// Target block (loop exit if allowed, error exit otherwise).
        target: BlockId,
        /// Whether this `break` is syntactically allowed here.
        allowed: bool,
    },
    /// `continue` within a loop.
    Continue {
        /// Span of the `continue`.
        span: Span,
        /// Target block (loop header if allowed, error exit otherwise).
        target: BlockId,
        /// Whether this `continue` is syntactically allowed here.
        allowed: bool,
    },
}

impl Terminator {
    /// Returns up to two successor blocks of this terminator.
    pub fn successors(&self) -> [Option<BlockId>; 2] {
        match *self {
            Terminator::Unset | Terminator::Exit(..) => [None, None],
            Terminator::Goto(bb) => [Some(bb), None],
            Terminator::Branch {
                then_bb, else_bb, ..
            } => [Some(then_bb), Some(else_bb)],
            Terminator::Return { target, .. }
            | Terminator::Break { target, .. }
            | Terminator::Continue { target, .. } => [Some(target), None],
        }
    }
}
