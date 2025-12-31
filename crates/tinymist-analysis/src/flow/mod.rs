//! Control-flow graph and dataflow analysis infrastructure.

/// Basic CFG graph primitives.
pub mod cfg;
/// Generic (forward/backward) dataflow solver.
pub mod dataflow;
/// Typst AST lowering into a statement-level CFG.
pub mod typst;
