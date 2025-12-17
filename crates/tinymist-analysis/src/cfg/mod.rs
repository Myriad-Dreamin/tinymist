//! Control-flow graph (CFG) construction and analysis for Typst syntax.
//!
//! This module builds CFGs directly from Typst's parsed AST (`typst::syntax::ast`),
//! so it can be used by both IDE features and linters/debug tooling.

mod analysis;
mod builder;
mod ipcfg;
mod ir;

#[cfg(test)]
mod tests;

pub use analysis::*;
pub use builder::*;
pub use ipcfg::*;
pub use ir::*;
