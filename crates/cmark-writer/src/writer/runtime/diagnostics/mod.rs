//! Diagnostics support for writer subsystems.
//!
//! This module introduces traits and lightweight collectors used by the
//! CommonMark/HTML writers to surface non-fatal warnings (e.g. best-effort
//! fallbacks) without hard depending on `log`.

use std::cell::RefCell;
use std::rc::Rc;

use ecow::EcoString;

/// Severity of a diagnostic entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Recoverable issues that do not stop rendering but may affect fidelity.
    Warning,
    /// Informational notes for downstream consumers.
    Info,
}

/// Diagnostic message emitted during rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity of the diagnostic.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: EcoString,
}

impl Diagnostic {
    /// Convenience constructor for warnings.
    pub fn warning<S: Into<EcoString>>(message: S) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
        }
    }

    /// Convenience constructor for informational diagnostics.
    pub fn info<S: Into<EcoString>>(message: S) -> Self {
        Self {
            severity: DiagnosticSeverity::Info,
            message: message.into(),
        }
    }
}

/// Trait used by writer subsystems to report non-fatal diagnostics.
pub trait DiagnosticSink {
    /// Emit a diagnostic message.
    fn emit(&mut self, diagnostic: Diagnostic);
}

/// A no-op sink used as default when the caller does not provide a collector.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl DiagnosticSink for NullSink {
    fn emit(&mut self, _: Diagnostic) {}
}

/// Shared sink that stores diagnostics in an `Rc<RefCell<Vec<Diagnostic>>>` for later inspection.
#[derive(Debug, Clone)]
pub struct SharedVecSink {
    target: Rc<RefCell<Vec<Diagnostic>>>,
}

impl SharedVecSink {
    /// Create a new shared sink backed by the supplied shared vector.
    pub fn new(target: Rc<RefCell<Vec<Diagnostic>>>) -> Self {
        Self { target }
    }

    /// Access the underlying shared storage.
    pub fn target(&self) -> Rc<RefCell<Vec<Diagnostic>>> {
        Rc::clone(&self.target)
    }
}

impl DiagnosticSink for SharedVecSink {
    fn emit(&mut self, diagnostic: Diagnostic) {
        self.target.borrow_mut().push(diagnostic);
    }
}
