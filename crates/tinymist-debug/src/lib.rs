//! Tinymist coverage support for Typst.

mod cov;
mod instrument;

use std::ops::DerefMut;
use std::sync::Arc;

use base64::Engine;
use parking_lot::Mutex;
use tinymist_std::{error::prelude::*, hash::FxHashMap};
use tinymist_world::package::PackageSpec;
use tinymist_world::{print_diagnostics, CompilerFeat, CompilerWorld};
use typst::diag::EcoString;
use typst::syntax::package::PackageVersion;
use typst::syntax::FileId;
use typst::utils::LazyHash;
use typst::{Library, WorldExt};

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
        // vector of file ids
        let mut file_ids: Vec<FileId> = self.regions.keys().cloned().collect();
        file_ids.sort_by(|a, b| {
            a.package()
                .map(PackageSpecCmp::from)
                .cmp(&b.package().map(PackageSpecCmp::from))
                .then_with(|| a.vpath().cmp(b.vpath()))
        });

        let mut regions = vec![];

        let mut file_meta = FxHashMap::default();

        for file_id in file_ids {
            let fid = {
                if let Some((index, _)) = file_meta.get(&file_id) {
                    *index
                } else {
                    let index = file_meta.len();

                    let meta = {
                        let meta = self.meta.get(&file_id).unwrap();

                        let pc_meta = meta
                            .meta
                            .as_slice()
                            .iter()
                            .flat_map(|(s, kind)| {
                                // todo: pool performance
                                let range = w.range(*s);
                                let kind = match kind {
                                    Kind::OpenBrace => 0,
                                    Kind::CloseBrace => 1,
                                    Kind::Functor => 2,
                                };

                                if let Some(range) = range {
                                    [kind, range.start as isize, range.end as isize]
                                } else {
                                    [kind, -1, -1]
                                }
                            })
                            .collect::<Vec<isize>>();

                        serde_json::json!({
                            "filePath": w.path_for_id(file_id).unwrap().as_path().to_str(),
                            "pcMeta": pc_meta,
                        })
                    };

                    file_meta.insert(file_id, (index, meta));

                    index
                }
            };

            let region = self.regions.get(&file_id).unwrap();
            let hits = base64::prelude::BASE64_STANDARD.encode(region.hits.lock().deref_mut());

            regions.push(serde_json::json!({
                "file_id": fid,
                "hits": hits,
            }));
        }

        let mut file_meta = file_meta.values().collect::<Vec<_>>();
        file_meta.sort_by(|a, b| a.0.cmp(&b.0));
        let file_meta = file_meta
            .into_iter()
            .map(|(_, meta)| meta)
            .collect::<Vec<_>>();

        serde_json::json!({
            "meta": file_meta,
            "regions": regions,
        })
    }
}

/// Collects the coverage of a single execution.
pub fn collect_coverage<D: typst::Document, F: CompilerFeat>(
    base: &CompilerWorld<F>,
) -> Result<CoverageResult> {
    let instr = InstrumentWorld {
        base,
        library: instrument_library(&base.library),
        instr: CoverageInstrumenter::default(),
        instrumented: Mutex::new(FxHashMap::default()),
    };

    let _cov_lock = cov::COVERAGE_LOCK.lock();

    if let Err(e) = typst::compile::<D>(&instr).output {
        print_diagnostics(&instr, e.iter(), tinymist_world::DiagnosticFormat::Human)
            .context_ut("failed to print diagnostics")?;
        bail!("");
    }

    let meta = std::mem::take(instr.instr.map.lock().deref_mut());
    let CoverageMap { regions, .. } = std::mem::take(cov::COVERAGE_MAP.lock().deref_mut());

    Ok(CoverageResult { meta, regions })
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
