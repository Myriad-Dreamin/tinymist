use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::bail;
use itertools::Itertools;
use log::info;
use lsp_types::*;
use serde::Deserialize;
use serde_json::{json, Map, Value as JsonValue};
use tinymist_query::{get_semantic_tokens_options, PositionEncoding};
use tokio::sync::mpsc;
use typst_ts_core::ImmutPath;

pub use super::lsp::LanguageState;
use super::*;
use crate::actor::editor::EditorActor;
use crate::compile_init::CompileConfig;
use crate::utils::{try_, try_or};
use crate::world::ImmutDict;

// todo: svelte-language-server responds to a Goto Definition request with
// LocationLink[] even if the client does not report the
// textDocument.definition.linkSupport capability.

/// The mode of the formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatterMode {
    /// Disable the formatter.
    #[default]
    Disable,
    /// Use `typstyle` formatter.
    Typstyle,
    /// Use `typstfmt` formatter.
    Typstfmt,
}

/// The mode of PDF/SVG/PNG export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportMode {
    #[default]
    Auto,
    /// Select best solution automatically. (Recommended)
    Never,
    /// Export on saving the document, i.e. on `textDocument/didSave` events.
    OnSave,
    /// Export on typing, i.e. on `textDocument/didChange` events.
    OnType,
    /// Export when a document has a title, which is useful to filter out
    /// template files.
    OnDocumentHasTitle,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompileExtraOpts {
    /// The root directory for compilation routine.
    pub root_dir: Option<PathBuf>,
    /// Path to entry
    pub entry: Option<ImmutPath>,
    /// Additional input arguments to compile the entry file.
    pub inputs: ImmutDict,
    /// will remove later
    pub font_paths: Vec<PathBuf>,
}

const CONFIG_ITEMS: &[&str] = &[
    "outputPath",
    "exportPdf",
    "rootPath",
    "semanticTokens",
    "formatterMode",
    "formatterPrintWidth",
    "fontPaths",
    "systemFonts",
    "typstExtraArgs",
    "compileStatus",
    "preferredTheme",
    "hoverPeriscope",
];

/// The user configuration read from the editor.
#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Specifies the root path of the project manually.
    pub notify_compile_status: bool,
    /// The compile configurations
    pub compile: CompileConfig,
    /// Dynamic configuration for semantic tokens.
    pub semantic_tokens: SemanticTokensMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter: FormatterMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter_print_width: u32,
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
        try_(|| SemanticTokensMode::deserialize(update.get("semanticTokens")?).ok())
            .inspect(|v| self.semantic_tokens = *v);
        try_(|| FormatterMode::deserialize(update.get("formatterMode")?).ok())
            .inspect(|v| self.formatter = *v);
        try_(|| u32::deserialize(update.get("formatterPrintWidth")?).ok())
            .inspect(|v| self.formatter_print_width = *v);
        self.compile.update_by_map(update)?;
        self.compile.validate()
    }
}

/// Configuration set at initialization that won't change within a single
/// session.
#[derive(Debug, Clone)]
pub struct ConstConfig {
    /// Determined position encoding, either UTF-8 or UTF-16.
    /// Defaults to UTF-16 if not specified.
    pub position_encoding: PositionEncoding,
    /// Allow dynamic registration of configuration changes.
    pub cfg_change_registration: bool,
    /// Allow dynamic registration of semantic tokens.
    pub tokens_dynamic_registration: bool,
    /// Allow overlapping tokens.
    pub tokens_overlapping_token_support: bool,
    /// Allow multiline tokens.
    pub tokens_multiline_token_support: bool,
    /// Allow line folding on documents.
    pub doc_line_folding_only: bool,
    /// Allow dynamic registration of document formatting.
    pub doc_fmt_dynamic_registration: bool,
}

impl From<&InitializeParams> for ConstConfig {
    fn from(params: &InitializeParams) -> Self {
        const DEFAULT_ENCODING: &[PositionEncodingKind] = &[PositionEncodingKind::UTF16];

        let position_encoding = {
            let general = params.capabilities.general.as_ref();
            let encodings = try_(|| Some(general?.position_encodings.as_ref()?.as_slice()));
            let encodings = encodings.unwrap_or(DEFAULT_ENCODING);

            if encodings.contains(&PositionEncodingKind::UTF8) {
                PositionEncoding::Utf8
            } else {
                PositionEncoding::Utf16
            }
        };

        let workspace = params.capabilities.workspace.as_ref();
        let doc = params.capabilities.text_document.as_ref();
        let sema = try_(|| doc?.semantic_tokens.as_ref());
        let fold = try_(|| doc?.folding_range.as_ref());
        let format = try_(|| doc?.formatting.as_ref());

        Self {
            position_encoding,
            cfg_change_registration: try_or(|| workspace?.configuration, false),
            tokens_dynamic_registration: try_or(|| sema?.dynamic_registration, false),
            tokens_overlapping_token_support: try_or(|| sema?.overlapping_token_support, false),
            tokens_multiline_token_support: try_or(|| sema?.multiline_token_support, false),
            doc_line_folding_only: try_or(|| fold?.line_folding_only, true),
            doc_fmt_dynamic_registration: try_or(|| format?.dynamic_registration, false),
        }
    }
}

pub struct Init {
    pub client: LspClient<LanguageState>,
    pub compile_opts: CompileFontOpts,
    pub exec_cmds: OnceLock<Vec<String>>,
}

impl Initializer for Init {
    type I = InitializeParams;
    type S = LanguageState;
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
    fn initialize(mut self, params: InitializeParams) -> (LanguageState, AnySchedulableResponse) {
        // self.tracing_init();

        // Initialize configurations
        let cc = ConstConfig::from(&params);
        info!("initialized with const_config {cc:?}");
        let mut config = Config {
            compile: CompileConfig {
                roots: match params.workspace_folders.as_ref() {
                    Some(roots) => roots
                        .iter()
                        .filter_map(|root| root.uri.to_file_path().ok())
                        .collect::<Vec<_>>(),
                    #[allow(deprecated)] // `params.root_path` is marked as deprecated
                    None => params
                        .root_uri
                        .as_ref()
                        .map(|uri| uri.to_file_path().unwrap())
                        .or_else(|| params.root_path.clone().map(PathBuf::from))
                        .into_iter()
                        .collect(),
                },
                font_opts: std::mem::take(&mut self.compile_opts),
                ..CompileConfig::default()
            },
            ..Config::default()
        };
        let res = match &params.initialization_options {
            Some(init) => config
                .update(init)
                .map_err(|e| e.to_string())
                .map_err(invalid_params),
            None => Ok(()),
        };

        // Bootstrap server
        let (editor_tx, editor_rx) = mpsc::unbounded_channel();

        let mut service = LanguageState::new(self.client.clone(), cc.clone(), editor_tx);

        if let Err(err) = res {
            return (service, Err(err));
        }

        info!("initialized with config {config:?}", config = config);
        service.primary.config = config.compile.clone();
        service.config = config;

        service.run_format_thread();
        service.run_user_action_thread();

        let editor_actor = EditorActor::new(
            self.client.clone(),
            editor_rx,
            service.config.compile.notify_compile_status,
        );

        service.primary.restart_server("primary");

        // Run the cluster in the background after we referencing it
        self.client.handle.spawn(editor_actor.run());

        // Respond to the host (LSP client)

        // Register these capabilities statically if the client does not support dynamic
        // registration
        let semantic_tokens_provider = match service.config.semantic_tokens {
            SemanticTokensMode::Enable if !cc.tokens_dynamic_registration => {
                Some(get_semantic_tokens_options().into())
            }
            _ => None,
        };
        let document_formatting_provider = match service.config.formatter {
            FormatterMode::Typstyle | FormatterMode::Typstfmt
                if !cc.doc_fmt_dynamic_registration =>
            {
                Some(OneOf::Left(true))
            }
            _ => None,
        };

        let res = InitializeResult {
            capabilities: ServerCapabilities {
                // todo: respect position_encoding
                // position_encoding: Some(cc.position_encoding.into()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    // Please update the language-configuration.json if you are changing this
                    // setting.
                    trigger_characters: Some(vec![
                        String::from("#"),
                        String::from("("),
                        String::from("<"),
                        String::from(","),
                        String::from("."),
                        String::from(":"),
                        String::from("/"),
                        String::from("\""),
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
                    commands: self.exec_cmds.get().unwrap().clone(),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
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
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),

                experimental: Some(json!({
                  "onEnter": true,
                })),
                ..Default::default()
            },
            ..Default::default()
        };

        let res = serde_json::to_value(res).map_err(|e| invalid_params(e.to_string()));
        (service, just_result!(res))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_update() {
        let mut config = Config::default();

        let root_path = if cfg!(windows) { "C:\\root" } else { "/root" };

        let update = json!({
            "outputPath": "out",
            "exportPdf": "onSave",
            "rootPath": root_path,
            "semanticTokens": "enable",
            "formatterMode": "typstyle",
            "typstExtraArgs": ["--root", root_path]
        });

        config.update(&update).unwrap();

        assert_eq!(config.compile.output_path, "out");
        assert_eq!(config.compile.export_pdf, ExportMode::OnSave);
        assert_eq!(config.compile.root_path, Some(PathBuf::from(root_path)));
        assert_eq!(config.semantic_tokens, SemanticTokensMode::Enable);
        assert_eq!(config.formatter, FormatterMode::Typstyle);
        assert_eq!(
            config.compile.typst_extra_args,
            Some(CompileExtraOpts {
                root_dir: Some(PathBuf::from(root_path)),
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_empty_extra_args() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": []
        });

        config.update(&update).unwrap();
    }

    #[test]
    fn test_reject_abnormal_root() {
        let mut config = Config::default();
        let update = json!({
            "rootPath": ".",
        });

        let err = format!("{}", config.update(&update).unwrap_err());
        assert!(err.contains("absolute path"), "unexpected error: {}", err);
    }

    #[test]
    fn test_reject_abnormal_root2() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": ["--root", "."]
        });

        let err = format!("{}", config.update(&update).unwrap_err());
        assert!(err.contains("absolute path"), "unexpected error: {}", err);
    }
}
