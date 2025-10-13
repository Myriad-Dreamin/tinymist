use lsp_types::*;
use reflexo_typst::Bytes;
use serde::{Deserialize, Serialize};
use tinymist_query::ty::PathKind;
use tinymist_query::{is_untitled_path, to_typst_range, PositionEncoding};
use tinymist_std::error::prelude::*;
use tinymist_std::ImmutPath;
use typst::ecow::EcoString;
use typst::{
    diag::{FileError, FileResult},
    syntax::Source,
};

use crate::project::Interrupt;
use crate::world::vfs::{notify::MemoryEvent, FileChangeSet, FilesystemEvent};
use crate::world::TaskInputs;
use crate::*;

mod watch;
pub use watch::WatchAccessModel;

#[cfg(feature = "lock")]
use crate::project::ProjectResolutionKind;
#[cfg(feature = "lock")]
use crate::route::ProjectResolution;

/// In memory source file management.
impl ServerState {
    /// Updates a set of source files.
    fn update_sources(&mut self, files: FileChangeSet) -> Result<()> {
        log::trace!("update source: {files:?}");

        let intr = Interrupt::Memory(MemoryEvent::Update(files.clone()));
        self.project.interrupt(intr);

        Ok(())
    }

    /// Creates a new source file.
    pub fn create_source(&mut self, path: ImmutPath, content: String) -> Result<()> {
        let _scope = typst_timing::TimingScope::new("create_source");
        log::trace!("create source: {path:?}");
        self.memory_changes
            .insert(path.clone(), Source::detached(content.clone()));

        let content = Bytes::from_string(content);

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok(content).into())]);

        self.update_sources(files)
    }

    /// Removes a source file.
    pub fn remove_source(&mut self, path: ImmutPath) -> Result<()> {
        let _scope = typst_timing::TimingScope::new("remove_source");
        self.memory_changes.remove(&path);
        log::trace!("remove source: {path:?}");

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(vec![path]);

        self.update_sources(files)
    }

    /// Edits a source file.
    pub fn edit_source(
        &mut self,
        path: ImmutPath,
        content: Vec<TextDocumentContentChangeEvent>,
        position_encoding: PositionEncoding,
    ) -> Result<()> {
        let _scope = typst_timing::TimingScope::new("edit_source");
        let source = self
            .memory_changes
            .get_mut(&path)
            .ok_or_else(|| error_once!("file missing", path: path.display()))?;

        for change in content {
            let replacement = change.text;
            match change.range {
                Some(lsp_range) => {
                    let range = to_typst_range(lsp_range, position_encoding, source)
                        .expect("invalid range");
                    source.edit(range, &replacement);
                }
                None => {
                    source.replace(&replacement);
                }
            }
        }

        let snapshot = FileResult::Ok(Bytes::from_string(source.text().to_owned())).into();

        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);

        self.update_sources(files)
    }

    /// Saves a source file.
    pub fn save_source(&mut self, path: ImmutPath) -> Result<()> {
        // FIXME: this is a workaround for the issue of notify, which does not fully
        // emit fs changes.
        //
        // The case to fix:
        // When editing a file, the last compilation sync deps to notify actor (rev=N-1,
        // actual state=S). At the time, the next fs change comes, and notifier actor
        // reads the change (notify state=S'). Next, another fs change comes (rev=N,
        // actual state=S'). However, since the notifier actor read the state
        // earlier (notify state=S'), the actor will not emit a change at rev N, bang...
        self.project.interrupt(Interrupt::Save(path));

        Ok(())
    }

    /// Queries a source file that must be in memory.
    pub fn query_source<T>(
        &self,
        path: ImmutPath,
        f: impl FnOnce(Source) -> LspResult<T>,
    ) -> LspResult<T> {
        let snapshot = self.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| internal_error(format!("file missing {path:?}")))?;
        let source = snapshot.clone();
        f(source)
    }

    /// Handles file system change event emitted by the client.
    pub fn fs_change(&mut self, params: FsChangeParams) -> ScheduleResult {
        use base64::Engine;
        let _scope = typst_timing::TimingScope::new("fs_change");

        let mut inserts = vec![];
        let mut removes = vec![];

        for change in params.inserts {
            let path: ImmutPath =
                tinymist_query::url_to_path(&Url::parse(&change.uri).unwrap()).into();
            let content: FileResult<Bytes> = match change.content {
                FileChangeResult::Ok { content } => base64::engine::general_purpose::STANDARD
                    .decode(content)
                    .map(Bytes::new)
                    .map_err(|e| FileError::Other(Some(_eco_format!("base64 decode error: {e}")))),
                FileChangeResult::Err { error } => {
                    log::info!("file content not available: {path:?} {error}");
                    Err(FileError::Other(Some(error)))
                }
            };
            inserts.push((path, content.into()));
        }

        for path in params.removes {
            removes.push(tinymist_query::url_to_path(&Url::parse(&path).unwrap()).into());
        }

        let update = FilesystemEvent::Update(FileChangeSet { inserts, removes }, params.is_sync);
        log::info!("fs_change: {update:?}");
        self.project.interrupt(Interrupt::Fs(update));

        self.schedule_async();
        just_ok(serde_json::Value::Null)
    }
}

/// Main file mutations on the primary project (which is used for the language
/// queries.)
impl ServerState {
    /// Updates the `pinning_by_preview` status.
    pub fn set_pin_by_preview(&mut self, pin: bool, browsing: bool) {
        self.pinning_by_preview = pin;
        self.pinning_by_browsing_preview = browsing;
    }

    /// Changes main file to the given path.
    pub fn change_main_file(&mut self, path: Option<ImmutPath>) -> Result<bool> {
        if path
            .as_deref()
            .is_some_and(|p| !p.is_absolute() && !p.starts_with("/untitled"))
        {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let task = self.resolve_task_or(path);

        log::info!("the task of the primary is changing to {task:?}");

        let id = self.project.primary_id().clone();
        self.project.interrupt(Interrupt::ChangeTask(id, task));

        Ok(true)
    }

    /// Pins the main file to the given path
    pub fn pin_main_file(&mut self, new_entry: Option<ImmutPath>) -> Result<()> {
        self.pinning_by_user = new_entry.is_some();
        let entry = new_entry
            .or_else(|| self.entry_resolver().resolve_default())
            .or_else(|| self.focusing.clone());

        self.change_main_file(entry).map(|_| ())
    }

    /// Focuses main file to the given path.
    pub fn focus_main_file(&mut self, new_entry: Option<ImmutPath>) -> Result<bool> {
        if new_entry.as_deref() != self.focusing.as_deref() {
            self.implicit_position = None;
        }

        self.focusing = new_entry.clone();

        if self.pinning_by_user
            || (self.pinning_by_preview && !self.pinning_by_browsing_preview)
            || self.config.has_default_entry_path
        {
            return Ok(false);
        }

        self.change_main_file(new_entry)
    }

    /// This is used for tracking activating document status if a client is not
    /// performing any focus command request.
    ///
    /// See <https://github.com/microsoft/language-server-protocol/issues/718>
    ///
    /// we do want to focus the file implicitly by `textDocument/diagnostic`
    /// (pullDiagnostics mode), as suggested by language-server-protocol#718,
    /// however, this has poor support, e.g. since Neovim 0.10.0.
    pub fn implicit_focus_entry(
        &mut self,
        new_entry: impl FnOnce() -> Option<ImmutPath>,
        site: char,
    ) {
        if self.ever_manual_focusing {
            return;
        }
        // didOpen
        match site {
            // foldingRange, hover, semanticTokens
            'f' | 'h' | 't' => {
                self.ever_focusing_by_activities = true;
            }
            // didOpen
            _ => {
                if self.ever_focusing_by_activities {
                    return;
                }
            }
        }

        let new_entry = new_entry();

        // Don't implicit focus non source file.
        if new_entry.as_ref().is_some_and(|entry| {
            !is_untitled_path(entry)
                && !PathKind::Source {
                    allow_package: true,
                }
                .is_match(entry)
        }) {
            return;
        }

        let update_result = self.focus_main_file(new_entry.clone());
        match update_result {
            Ok(true) => {
                log::info!("file focused[implicit,{site}]: {new_entry:?}");
            }
            Err(err) => {
                log::warn!("could not focus file: {err}");
            }
            Ok(false) => {}
        }
    }
}

/// Task input resolution.
impl ServerState {
    fn resolve_task_without_lock(&self, path: Option<ImmutPath>) -> TaskInputs {
        TaskInputs {
            entry: Some(self.entry_resolver().resolve(path)),
            ..TaskInputs::default()
        }
    }

    pub(crate) fn resolve_task_or(&mut self, path: Option<ImmutPath>) -> TaskInputs {
        path.clone()
            .map(|path| self.resolve_task(path))
            .unwrap_or_else(|| self.resolve_task_without_lock(path))
    }

    #[cfg(not(feature = "lock"))]
    pub(crate) fn resolve_task(&mut self, path: ImmutPath) -> TaskInputs {
        self.resolve_task_without_lock(Some(path))
    }

    #[cfg(feature = "lock")]
    pub(crate) fn resolve_task(&mut self, path: ImmutPath) -> TaskInputs {
        let proj_input = matches!(
            self.entry_resolver().project_resolution,
            ProjectResolutionKind::LockDatabase
        )
        .then(|| {
            let resolution = self.route.resolve(&path)?;
            let lock = self.route.locate(&resolution)?;

            let ProjectResolution {
                lock_dir,
                project_id,
            } = &resolution;

            let input = lock.get_document(project_id)?;
            let root = input
                .root
                .as_ref()
                .and_then(|res| Some(res.to_abs_path(lock_dir)?.as_path().into()))
                .unwrap_or_else(|| lock_dir.clone());
            let main = input
                .main
                .to_abs_path(lock_dir)
                .map(|path| path.as_path().into())
                .unwrap_or_else(|| path.clone());
            let entry = self
                .entry_resolver()
                .resolve_with_root(Some(root), Some(main));
            log::info!("resolved task with state: {path:?} -> {project_id:?} -> {entry:?}");

            Some(TaskInputs {
                entry: Some(entry),
                ..TaskInputs::default()
            })
        });

        proj_input
            .flatten()
            .unwrap_or_else(|| self.resolve_task_without_lock(Some(path)))
    }
}

/// The file system change request.
pub enum FsChange {}

impl lsp_types::request::Request for FsChange {
    type Params = FsChangeParams;
    type Result = ();
    const METHOD: &'static str = "tinymist/fsChange";
}

/// The file system change parameters.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsChangeParams {
    /// The inserted files.
    pub inserts: Vec<FileChange>,
    /// The removed files.
    pub removes: Vec<String>,
    /// Whether the change is emitted by sync event.
    pub is_sync: bool,
}

/// A file change.

#[derive(Debug, Serialize, Deserialize)]
pub struct FileChange {
    /// The file URI.
    pub uri: String,
    /// The file content.
    pub content: FileChangeResult,
}

/// The result of a file change.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FileChangeResult {
    /// The file content is available.
    Ok {
        /// The file content.
        content: String,
    },
    /// The file content is not available.
    Err {
        /// The error message.
        error: EcoString,
    },
}
