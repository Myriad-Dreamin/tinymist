use std::{collections::HashMap, path::Path, sync::Arc};

use parking_lot::Mutex;
use reflexo::ImmutPath;
use reflexo_typst::TypstFileId;
use tinymist_world::LspWorld;
use typst::{diag::FileResult, syntax::Source};

use crate::LspWorldExt;

#[salsa::input]
pub struct SalsaSource {
    pub path: ImmutPath,
    #[return_ref]
    pub contents: Source,
}

pub trait PathResolver {
    /// Get file's id by its path
    fn file_id_by_path(&self, p: &Path) -> FileResult<TypstFileId>;
}

/// Database which stores all significant input facts: source code and project
/// model. Everything else in tinymist is derived from these queries.
#[salsa_macros::db]
pub trait Db: salsa::Database + PathResolver {
    fn source_by_path(&self, path: ImmutPath) -> FileResult<SalsaSource>;
}

#[salsa::db]
#[derive(Clone)]
pub struct SourceDb {
    // src.set_contents(db).to();.
    pub sources: HashMap<ImmutPath, SalsaSource>,
    pub world: Arc<LspWorld>,

    storage: salsa::Storage<Self>,
    // The logs are only used for testing and demonstrating reuse:
    logs: Arc<Mutex<Option<Vec<String>>>>,
}

impl PathResolver for SourceDb {
    fn file_id_by_path(&self, p: &Path) -> FileResult<TypstFileId> {
        self.world.file_id_by_path(p)
    }
}

impl salsa::Database for SourceDb {
    fn zalsa_db(&self) {}

    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        let event = event();
        eprintln!("Event: {event:?}");
        // Log interesting events, if logging is enabled
        if let Some(logs) = &mut *self.logs.lock() {
            // only log interesting events
            if let salsa::EventKind::WillExecute { .. } = event.kind {
                logs.push(format!("Event: {event:?}"));
            }
        }
    }
}

impl Db for SourceDb {
    fn zalsa_db(&self) {}

    fn source_by_path(&self, path: ImmutPath) -> FileResult<SalsaSource> {
        Ok(SalsaSource::new(
            self,
            path.clone(),
            self.world.source_by_path(&path)?,
        ))
    }
}
