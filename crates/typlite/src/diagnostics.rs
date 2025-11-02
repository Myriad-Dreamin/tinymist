use std::sync::{Arc, Mutex};

use log::warn;
use tinymist_project::diag::print_diagnostics_to_string;
use tinymist_project::{DiagnosticFormat, SourceWorld};
use typst::diag::SourceDiagnostic;

/// Shared collector for Typst warnings emitted during conversion.
#[derive(Clone, Default)]
pub(crate) struct WarningCollector {
    inner: Arc<Mutex<Vec<SourceDiagnostic>>>,
}

impl WarningCollector {
    /// Extend the collector with multiple warnings.
    pub fn extend<I>(&self, warnings: I)
    where
        I: IntoIterator<Item = SourceDiagnostic>,
    {
        let mut guard = self.inner.lock().expect("warning collector poisoned");
        guard.extend(warnings);
    }

    /// Clone all collected warnings into a standalone vector.
    pub fn snapshot(&self) -> Vec<SourceDiagnostic> {
        let guard = self.inner.lock().expect("warning collector poisoned");
        guard.clone()
    }
}

/// Render warnings into a human-readable string for the CLI.
#[allow(dead_code)]
pub(crate) fn render_warnings<'a>(
    world: &dyn SourceWorld,
    warnings: impl IntoIterator<Item = &'a SourceDiagnostic>,
) -> Option<String> {
    let warnings: Vec<&SourceDiagnostic> = warnings.into_iter().collect();
    if warnings.is_empty() {
        return None;
    }

    match print_diagnostics_to_string(world, warnings.into_iter(), DiagnosticFormat::Human) {
        Ok(message) => Some(message.to_string()),
        Err(err) => {
            warn!("failed to render Typst warnings: {err}");
            None
        }
    }
}
