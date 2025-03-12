//! Tinymist coverage support for Typst.

mod cov;
mod instrument;

use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::Arc;

use parking_lot::Mutex;
use tinymist_analysis::location::PositionEncoding;
use tinymist_std::debug_loc::LspRange;
use tinymist_std::{error::prelude::*, hash::FxHashMap};
use tinymist_world::package::PackageSpec;
use tinymist_world::{print_diagnostics, CompilerFeat, CompilerWorld};
use typst::diag::EcoString;
use typst::syntax::package::PackageVersion;
use typst::syntax::FileId;
use typst::utils::LazyHash;
use typst::{Library, World, WorldExt};

use cov::*;
use instrument::InstrumentWorld;

/// The coverage result.
pub struct CoverageResult {
    /// The coverage meta.
    pub meta: FxHashMap<FileId, Arc<InstrumentMeta>>,
    /// The coverage map.
    pub regions: FxHashMap<FileId, CovRegion>,
}

impl CoverageResult {
    /// Converts the coverage result to JSON.
    pub fn to_json<F: CompilerFeat>(&self, w: &CompilerWorld<F>) -> serde_json::Value {
        let lsp_position_encoding = PositionEncoding::Utf16;

        let mut result = VscodeCoverage::new();

        for (file_id, region) in &self.regions {
            let file_path = w
                .path_for_id(*file_id)
                .unwrap()
                .as_path()
                .to_str()
                .unwrap()
                .to_string();

            let mut details = vec![];

            let meta = self.meta.get(file_id).unwrap();

            let Ok(typst_source) = w.source(*file_id) else {
                continue;
            };

            let hits = region.hits.lock();
            for (idx, (span, _kind)) in meta.meta.iter().enumerate() {
                let Some(typst_range) = w.range(*span) else {
                    continue;
                };

                let rng = tinymist_analysis::location::to_lsp_range(
                    typst_range,
                    &typst_source,
                    lsp_position_encoding,
                );

                details.push(VscodeFileCoverageDetail {
                    executed: hits[idx] > 0,
                    location: rng,
                });
            }

            result.insert(file_path, details);
        }

        serde_json::to_value(result).unwrap()
    }
}

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
pub fn with_cov<F: CompilerFeat>(
    base: &CompilerWorld<F>,
    mut f: impl FnMut(&InstrumentWorld<F, CoverageInstrumenter>) -> Result<()>,
) -> (Result<CoverageResult>, Result<()>) {
    let instr = InstrumentWorld {
        base,
        library: instrument_library(&base.library),
        instr: CoverageInstrumenter::default(),
        instrumented: Mutex::new(FxHashMap::default()),
    };

    let _cov_lock = cov::COVERAGE_LOCK.lock();

    let result = f(&instr);

    let meta = std::mem::take(instr.instr.map.lock().deref_mut());
    let CoverageMap { regions, .. } = std::mem::take(cov::COVERAGE_MAP.lock().deref_mut());

    (Ok(CoverageResult { meta, regions }), result)
}

#[comemo::memoize]
fn instrument_library(library: &Arc<LazyHash<Library>>) -> Arc<LazyHash<Library>> {
    let mut library = library.as_ref().clone();

    library.global.scope_mut().define_func::<__cov_pc>();
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

/// The coverage result in the format of the VSCode coverage data.
pub type VscodeCoverage = HashMap<String, Vec<VscodeFileCoverageDetail>>;

/// Converts the coverage result to the VSCode coverage data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VscodeFileCoverageDetail {
    /// Whether the location is being executed
    pub executed: bool,
    /// The location of the coverage.
    pub location: LspRange,
}
