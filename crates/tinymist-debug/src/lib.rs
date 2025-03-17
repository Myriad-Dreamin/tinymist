//! Tinymist coverage support for Typst.

pub use debugger::{
    set_debug_session, with_debug_session, BreakpointKind, DebugSession, DebugSessionHandler,
};

mod cov;
mod debugger;
mod instrument;

use std::ops::DerefMut;
use std::sync::Arc;

use debugger::BreakpointInstr;
use parking_lot::Mutex;
use tinymist_std::{error::prelude::*, hash::FxHashMap};
use tinymist_world::package::PackageSpec;
use tinymist_world::{print_diagnostics, CompilerFeat, CompilerWorld};
use typst::diag::EcoString;
use typst::syntax::package::PackageVersion;
use typst::utils::LazyHash;
use typst::Library;

use cov::*;
use instrument::InstrumentWorld;

/// Collects the coverage of a single execution.
pub fn collect_coverage<D: typst::Document, F: CompilerFeat>(
    base: &CompilerWorld<F>,
) -> Result<CoverageResult> {
    let (cov, result) = with_cov(base, |instr| {
        if let Err(e) = typst::compile::<D>(&instr).output {
            print_diagnostics(instr, e.iter(), tinymist_world::DiagnosticFormat::Human)
                .context_ut("failed to print diagnostics")?;
            bail!("");
        }

        Ok(())
    });

    result?;
    cov
}

/// Collects the coverage with a callback.
pub fn with_cov<F: CompilerFeat, T>(
    base: &CompilerWorld<F>,
    mut f: impl FnMut(&InstrumentWorld<F, CovInstr>) -> Result<T>,
) -> (Result<CoverageResult>, Result<T>) {
    let instr = InstrumentWorld {
        base,
        library: instrument_library(&base.library),
        instr: CovInstr::default(),
        instrumented: Mutex::new(FxHashMap::default()),
    };

    let _cov_lock = cov::COVERAGE_LOCK.lock();

    let result = f(&instr);

    let meta = std::mem::take(instr.instr.map.lock().deref_mut());
    let CoverageMap { regions, .. } = std::mem::take(cov::COVERAGE_MAP.lock().deref_mut());

    (Ok(CoverageResult { meta, regions }), result)
}

/// The world for debugging.
pub type DebuggerWorld<'a, F> = InstrumentWorld<'a, F, BreakpointInstr>;
/// Creates a world for debugging.
pub fn instr_breakpoints<F: CompilerFeat>(base: &CompilerWorld<F>) -> DebuggerWorld<'_, F> {
    InstrumentWorld {
        base,
        library: instrument_library(&base.library),
        instr: BreakpointInstr::default(),
        instrumented: Mutex::new(FxHashMap::default()),
    }
}

#[comemo::memoize]
fn instrument_library(library: &Arc<LazyHash<Library>>) -> Arc<LazyHash<Library>> {
    use debugger::breakpoints::*;

    let mut library = library.as_ref().clone();

    let scope = library.global.scope_mut();
    scope.define_func::<__cov_pc>();
    scope.define_func::<__breakpoint_call_start>();
    scope.define_func::<__breakpoint_call_end>();
    scope.define_func::<__breakpoint_function>();
    scope.define_func::<__breakpoint_break>();
    scope.define_func::<__breakpoint_continue>();
    scope.define_func::<__breakpoint_return>();
    scope.define_func::<__breakpoint_block_start>();
    scope.define_func::<__breakpoint_block_end>();
    scope.define_func::<__breakpoint_show_start>();
    scope.define_func::<__breakpoint_show_end>();
    scope.define_func::<__breakpoint_doc_start>();
    scope.define_func::<__breakpoint_doc_end>();

    scope.define_func::<__breakpoint_call_start_handle>();
    scope.define_func::<__breakpoint_call_end_handle>();
    scope.define_func::<__breakpoint_function_handle>();
    scope.define_func::<__breakpoint_break_handle>();
    scope.define_func::<__breakpoint_continue_handle>();
    scope.define_func::<__breakpoint_return_handle>();
    scope.define_func::<__breakpoint_block_start_handle>();
    scope.define_func::<__breakpoint_block_end_handle>();
    scope.define_func::<__breakpoint_show_start_handle>();
    scope.define_func::<__breakpoint_show_end_handle>();
    scope.define_func::<__breakpoint_doc_start_handle>();
    scope.define_func::<__breakpoint_doc_end_handle>();

    Arc::new(library)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PackageSpecCmp<'a> {
    /// The namespace the package lives in.
    pub namespace: &'a EcoString,
    /// The name of the package within its namespace.
    pub name: &'a EcoString,
    /// The package's version.
    pub version: &'a PackageVersion,
}

impl<'a> From<&'a PackageSpec> for PackageSpecCmp<'a> {
    fn from(spec: &'a PackageSpec) -> Self {
        Self {
            namespace: &spec.namespace,
            name: &spec.name,
            version: &spec.version,
        }
    }
}
