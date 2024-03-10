use core::fmt;
use std::{collections::HashMap, path::PathBuf};

use anyhow::bail;
use itertools::Itertools;
use lsp_types::*;
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{get_semantic_tokens_options, PositionEncoding};
use tokio::sync::mpsc;

use crate::actor::cluster::CompileClusterActor;
use crate::{invalid_params, LspHost, LspResult, TypstLanguageServer};

trait InitializeParamsExt {
    fn position_encodings(&self) -> &[PositionEncodingKind];
    fn supports_config_change_registration(&self) -> bool;
    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities>;
    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities>;
    fn supports_semantic_tokens_dynamic_registration(&self) -> bool;
    fn supports_document_formatting_dynamic_registration(&self) -> bool;
    fn line_folding_only(&self) -> bool;
    fn root_paths(&self) -> Vec<PathBuf>;
}

impl InitializeParamsExt for InitializeParams {
    fn position_encodings(&self) -> &[PositionEncodingKind] {
        const DEFAULT_ENCODING: &[PositionEncodingKind; 1] = &[PositionEncodingKind::UTF16];
        self.capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .map(|encodings| encodings.as_slice())
            .unwrap_or(DEFAULT_ENCODING)
    }

    fn supports_config_change_registration(&self) -> bool {
        self.capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.configuration)
            .unwrap_or(false)
    }

    fn line_folding_only(&self) -> bool {
        self.capabilities
            .text_document
            .as_ref()
            .and_then(|workspace| workspace.folding_range.as_ref())
            .and_then(|folding| folding.line_folding_only)
            .unwrap_or(false)
    }

    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .semantic_tokens
            .as_ref()
    }

    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .formatting
            .as_ref()
    }

    fn supports_semantic_tokens_dynamic_registration(&self) -> bool {
        self.semantic_tokens_capabilities()
            .and_then(|semantic_tokens| semantic_tokens.dynamic_registration)
            .unwrap_or(false)
    }

    fn supports_document_formatting_dynamic_registration(&self) -> bool {
        self.document_formatting_capabilities()
            .and_then(|document_format| document_format.dynamic_registration)
            .unwrap_or(false)
    }

    #[allow(deprecated)] // `self.root_path` is marked as deprecated
    fn root_paths(&self) -> Vec<PathBuf> {
        match self.workspace_folders.as_ref() {
            Some(roots) => roots
                .iter()
                .map(|root| &root.uri)
                .map(Url::to_file_path)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            None => self
                .root_uri
                .as_ref()
                .map(|uri| uri.to_file_path().unwrap())
                .or_else(|| self.root_path.clone().map(PathBuf::from))
                .into_iter()
                .collect(),
        }
    }
}

/// The mode of the experimental formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentalFormatterMode {
    /// Disable the experimental formatter.
    #[default]
    Disable,
    /// Enable the experimental formatter.
    Enable,
}

/// The mode of PDF export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportPdfMode {
    /// Don't export PDF automatically.
    Never,
    /// Export PDF on saving the document, i.e. on `textDocument/didSave`
    /// events.
    #[default]
    OnSave,
    /// Export PDF on typing, i.e. on `textDocument/didChange` events.
    OnType,
}

/// The mode of semantic tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticTokensMode {
    /// Disable the semantic tokens.
    Disable,
    /// Enable the semantic tokens.
    #[default]
    Enable,
}

type Listener<T> = Box<dyn FnMut(&T) -> anyhow::Result<()>>;

const CONFIG_ITEMS: &[&str] = &[
    "exportPdf",
    "rootPath",
    "semanticTokens",
    "experimentalFormatterMode",
];

/// The user configuration read from the editor.
#[derive(Default)]
pub struct Config {
    /// The mode of PDF export.
    pub export_pdf: ExportPdfMode,
    /// Specifies the root path of the project manually.
    pub root_path: Option<PathBuf>,
    /// Dynamic configuration for semantic tokens.
    pub semantic_tokens: SemanticTokensMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter: ExperimentalFormatterMode,
    semantic_tokens_listeners: Vec<Listener<SemanticTokensMode>>,
    formatter_listeners: Vec<Listener<ExperimentalFormatterMode>>,
}

impl Config {
    /// Gets items for serialization.
    pub fn get_items() -> Vec<ConfigurationItem> {
        let sections = CONFIG_ITEMS
            .iter()
            .flat_map(|item| [format!("tinymist.{item}"), item.to_string()]);

        sections
            .map(|section| ConfigurationItem {
                section: Some(section),
                ..Default::default()
            })
            .collect()
    }

    /// Converts values to a map.
    pub fn values_to_map(values: Vec<JsonValue>) -> Map<String, JsonValue> {
        let unpaired_values = values
            .into_iter()
            .tuples()
            .map(|(a, b)| if !a.is_null() { a } else { b });

        CONFIG_ITEMS
            .iter()
            .map(|item| item.to_string())
            .zip(unpaired_values)
            .collect()
    }

    /// Updates the configuration with a JSON object.
    ///
    /// # Errors
    /// Errors if the update is invalid.
    pub fn update(&mut self, update: &JsonValue) -> anyhow::Result<()> {
        if let JsonValue::Object(update) = update {
            self.update_by_map(update)
        } else {
            bail!("got invalid configuration object {update}")
        }
    }

    /// Updates the configuration with a map.
    ///
    /// # Errors
    /// Errors if the update is invalid.
    pub fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> anyhow::Result<()> {
        let export_pdf = update
            .get("exportPdf")
            .map(ExportPdfMode::deserialize)
            .and_then(Result::ok);
        if let Some(export_pdf) = export_pdf {
            self.export_pdf = export_pdf;
        }

        let root_path = update.get("rootPath");
        if let Some(root_path) = root_path {
            if root_path.is_null() {
                self.root_path = None;
            }
            if let Some(root_path) = root_path.as_str().map(PathBuf::from) {
                self.root_path = Some(root_path);
            }
        }

        let semantic_tokens = update
            .get("semanticTokens")
            .map(SemanticTokensMode::deserialize)
            .and_then(Result::ok);
        if let Some(semantic_tokens) = semantic_tokens {
            for listener in &mut self.semantic_tokens_listeners {
                listener(&semantic_tokens)?;
            }
            self.semantic_tokens = semantic_tokens;
        }

        let formatter = update
            .get("experimentalFormatterMode")
            .map(ExperimentalFormatterMode::deserialize)
            .and_then(Result::ok);
        if let Some(formatter) = formatter {
            for listener in &mut self.formatter_listeners {
                listener(&formatter)?;
            }
            self.formatter = formatter;
        }

        Ok(())
    }

    pub(crate) fn listen_semantic_tokens(&mut self, listener: Listener<SemanticTokensMode>) {
        self.semantic_tokens_listeners.push(listener);
    }

    // pub fn listen_formatting(&mut self, listener:
    // Listener<ExperimentalFormatterMode>) {     self.formatter_listeners.
    // push(listener); }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Config")
            .field("export_pdf", &self.export_pdf)
            .field("formatter", &self.formatter)
            .field("semantic_tokens", &self.semantic_tokens)
            .field(
                "semantic_tokens_listeners",
                &format_args!("Vec[len = {}]", self.semantic_tokens_listeners.len()),
            )
            .field(
                "formatter_listeners",
                &format_args!("Vec[len = {}]", self.formatter_listeners.len()),
            )
            .finish()
    }
}

/// Configuration set at initialization that won't change within a single
/// session
#[derive(Debug, Clone)]
pub struct ConstConfig {
    /// The position encoding, either UTF-8 or UTF-16.
    /// Defaults to UTF-16 if not specified.
    pub position_encoding: PositionEncoding,
    /// Whether the client supports dynamic registration of semantic tokens.
    pub supports_semantic_tokens_dynamic_registration: bool,
    /// Whether the client supports dynamic registration of document formatting.
    pub supports_document_formatting_dynamic_registration: bool,
    /// Whether the client supports dynamic registration of configuration
    /// changes.
    pub supports_config_change_registration: bool,
    /// Whether the client only supports line folding.
    pub line_folding_only: bool,
}

impl ConstConfig {
    fn choose_encoding(params: &InitializeParams) -> PositionEncoding {
        let encodings = params.position_encodings();
        if encodings.contains(&PositionEncodingKind::UTF8) {
            PositionEncoding::Utf8
        } else {
            PositionEncoding::Utf16
        }
    }
}

impl From<&InitializeParams> for ConstConfig {
    fn from(params: &InitializeParams) -> Self {
        Self {
            position_encoding: Self::choose_encoding(params),
            supports_semantic_tokens_dynamic_registration: params
                .supports_semantic_tokens_dynamic_registration(),
            supports_document_formatting_dynamic_registration: params
                .supports_document_formatting_dynamic_registration(),
            supports_config_change_registration: params.supports_config_change_registration(),
            line_folding_only: params.line_folding_only(),
        }
    }
}

pub struct Init {
    pub host: LspHost,
}

impl Init {
    /// The [`initialize`] request is the first request sent from the client to
    /// the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this
    /// request to the server again, the server will respond with JSON-RPC
    /// error code `-32600` (invalid request).
    ///
    /// # Panics
    /// Panics if the const configuration is already initialized.
    /// Panics if the cluster is already initialized.
    ///
    /// # Errors
    /// Errors if the configuration could not be updated.
    pub fn initialize(
        self,
        params: InitializeParams,
    ) -> (TypstLanguageServer, LspResult<InitializeResult>) {
        // self.tracing_init();

        // Initialize configurations
        let cc = ConstConfig::from(&params);
        let mut config = Config::default();

        // Bootstrap server
        let (diag_tx, diag_rx) = mpsc::unbounded_channel();

        let service =
            TypstLanguageServer::new(self.host.clone(), params.root_paths(), &cc, diag_tx);

        if let Some(init) = &params.initialization_options {
            if let Err(err) = config
                .update(init)
                .as_ref()
                .map_err(ToString::to_string)
                .map_err(invalid_params)
            {
                return (service, Err(err));
            }
        }

        let cluster_actor = CompileClusterActor {
            host: self.host.clone(),
            diag_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
            published_primary: false,
        };

        let primary = service.server("primary".to_owned(), None);
        service.primary.get_or_init(|| primary);

        // Run the cluster in the background after we referencing it
        tokio::spawn(cluster_actor.run());

        // Respond to the host (LSP client)
        let semantic_tokens_provider = match config.semantic_tokens {
            SemanticTokensMode::Enable
                if !params.supports_semantic_tokens_dynamic_registration() =>
            {
                Some(get_semantic_tokens_options().into())
            }
            _ => None,
        };

        let document_formatting_provider = match config.formatter {
            ExperimentalFormatterMode::Enable
                if !params.supports_document_formatting_dynamic_registration() =>
            {
                Some(OneOf::Left(true))
            }
            _ => None,
        };

        let res = InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        String::from("#"),
                        String::from("."),
                        String::from("@"),
                    ]),
                    ..Default::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                semantic_tokens_provider,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: service.exec_cmds.keys().map(ToString::to_string).collect(),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                })),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    ..Default::default()
                }),
                document_formatting_provider,
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        };

        (service, Ok(res))
    }
}
