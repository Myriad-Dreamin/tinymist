//! Runtime utilities shared between different writer backends.

pub mod diagnostics;
/// Restricted writer adapters exposed to custom node implementations.
pub mod proxy;
pub mod visitor;
