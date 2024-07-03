use std::{collections::HashMap, path::Path, sync::Arc};

use compile_init::CompileInit;
use log::{error, info};
use once_cell::sync::OnceCell;
use serde_json::{Map, Value as JsonValue};
use tokio::sync::mpsc;
use typst::{diag::FileResult, syntax::Source};
use typst_ts_compiler::vfs::notify::FileChangeSet;
use typst_ts_core::config::compiler::DETACHED_ENTRY;

use super::*;
use crate::{
    actor::{editor::EditorRequest, export::ExportConfig, typ_client::CompileClientActor},
    compile_init::{CompileConfig, ConstCompileConfig},
    state::MemoryFileMeta,
};

/// The object providing the language server functionality.
pub struct CompileState {
    /// The lsp client
    pub client: TypedLspClient<Self>,

    // State to synchronize with the client.
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,

    // Configurations
    /// User configuration from the editor.
    pub config: CompileConfig,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: ConstCompileConfig,

    // Resources
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, MemoryFileMeta>,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub editor_tx: mpsc::UnboundedSender<EditorRequest>,
    /// The compiler actor.
    pub compiler: Option<CompileClientActor>,
}

impl CompileState {
    pub fn new(
        client: TypedLspClient<CompileState>,
        compile_config: CompileConfig,
        const_config: ConstCompileConfig,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
    ) -> Self {
        CompileState {
            client,
            editor_tx,
            shutdown_requested: false,
            config: compile_config,
            const_config,
            compiler: None,
            memory_changes: HashMap::new(),
        }
    }

    pub fn install(provider: LspBuilder<CompileInit>) -> LspBuilder<CompileInit> {
        type S = CompileState;
        use lsp_types::notification::*;
        provider
            .with_command_("tinymist.exportPdf", S::export_pdf)
            .with_command_("tinymist.exportSvg", S::export_svg)
            .with_command_("tinymist.exportPng", S::export_png)
            .with_command("tinymist.doClearCache", S::clear_cache)
            .with_command("tinymist.changeEntry", S::change_entry)
            .with_notification::<Initialized>(S::initialized)
    }

    pub fn const_config(&self) -> &ConstCompileConfig {
        &self.const_config
    }

    pub fn compiler(&self) -> &CompileClientActor {
        self.compiler.as_ref().unwrap()
    }

    pub fn vfs_snapshot(&self) -> FileChangeSet {
        FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, meta)| {
                    let content = meta.content.clone().text().as_bytes().into();
                    (path.clone(), FileResult::Ok((meta.mt, content)).into())
                })
                .collect(),
        )
    }

    pub fn apply_vfs_snapshot(&mut self, changeset: FileChangeSet) {
        for path in changeset.removes {
            self.memory_changes.remove(&path);
        }

        for (path, file) in changeset.inserts {
            let Ok(content) = file.content() else {
                continue;
            };
            let Ok(mtime) = file.mtime() else {
                continue;
            };
            let Ok(content) = std::str::from_utf8(content) else {
                log::error!("invalid utf8 content in snapshot file: {path:?}");
                continue;
            };

            let meta = MemoryFileMeta {
                mt: *mtime,
                content: Source::new(*DETACHED_ENTRY, content.to_owned()),
            };
            self.memory_changes.insert(path, meta);
        }
    }
}

impl CompileState {
    pub fn on_changed_configuration(&mut self, values: Map<String, JsonValue>) -> LspResult<()> {
        let config = self.config.clone();
        match self.config.update_by_map(&values) {
            Ok(()) => {}
            Err(err) => {
                self.config = config;
                error!("error applying new settings: {err}");
                return Err(invalid_params(format!(
                    "error applying new settings: {err}"
                )));
            }
        }

        if let Some(e) = self.compiler.as_mut() {
            e.sync_config(self.config.clone());
        }

        if config.output_path != self.config.output_path
            || config.export_pdf != self.config.export_pdf
        {
            let config = ExportConfig {
                substitute_pattern: self.config.output_path.clone(),
                mode: self.config.export_pdf,
            };

            self.compiler
                .as_mut()
                .unwrap()
                .change_export_pdf(config.clone());
        }

        if config.primary_opts() != self.config.primary_opts() {
            self.config.fonts = OnceCell::new(); // todo: don't reload fonts if not changed
            self.restart_server("primary");
        }

        info!("new settings applied");
        Ok(())
    }

    pub fn initialized(&mut self, _params: lsp_types::InitializedParams) -> LspResult<()> {
        Ok(())
    }
}
