use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use clap::Parser;
use comemo::Prehashed;
use itertools::Itertools;
use lsp_types::*;
use once_cell::sync::{Lazy, OnceCell};
use reflexo::path::PathClean;
use reflexo_typst::font::FontResolverImpl;
use reflexo_typst::world::EntryState;
use reflexo_typst::{ImmutPath, TypstDict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use task::FormatUserConfig;
use tinymist_query::{get_semantic_tokens_options, PositionEncoding};
use tinymist_render::PeriscopeArgs;
use typst::foundations::IntoValue;
use typst::syntax::{FileId, VirtualPath};
use typst::util::Deferred;

// todo: svelte-language-server responds to a Goto Definition request with
// LocationLink[] even if the client does not report the
// textDocument.definition.linkSupport capability.

use super::*;
use crate::world::ImmutDict;

/// Capability to add valid commands to the arguments.
pub trait AddCommands {
    /// Adds commands to the arguments.
    fn add_commands(&mut self, cmds: &[String]);
}

/// The regular initializer.
pub struct RegularInit {
    /// The connection to the client.
    pub client: TypedLspClient<LanguageState>,
    /// The font options for the compiler.
    pub font_opts: CompileFontArgs,
    /// The commands to execute.
    pub exec_cmds: Vec<String>,
}

impl AddCommands for RegularInit {
    fn add_commands(&mut self, cmds: &[String]) {
        self.exec_cmds.extend(cmds.iter().cloned());
    }
}

impl Initializer for RegularInit {
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
        // Initialize configurations
        let mut config = Config {
            const_config: ConstConfig::from(&params),
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
                font_opts: std::mem::take(&mut self.font_opts),
                ..CompileConfig::default()
            },
            ..Config::default()
        };
        let err = params.initialization_options.and_then(|init| {
            config
                .update(&init)
                .map_err(|e| e.to_string())
                .map_err(invalid_params)
                .err()
        });

        let super_init = SuperInit {
            client: self.client,
            exec_cmds: self.exec_cmds,
            config,
            err,
        };

        super_init.initialize(())
    }
}

/// The super LSP initializer.
pub struct SuperInit {
    /// Using the connection to the client.
    pub client: TypedLspClient<LanguageState>,
    /// The valid commands for `workspace/executeCommand` requests.
    pub exec_cmds: Vec<String>,
    /// The configuration for the server.
    pub config: Config,
    /// Whether an error occurred before super initialization.
    pub err: Option<ResponseError>,
}

impl AddCommands for SuperInit {
    fn add_commands(&mut self, cmds: &[String]) {
        self.exec_cmds.extend(cmds.iter().cloned());
    }
}

impl Initializer for SuperInit {
    type I = ();
    type S = LanguageState;
    fn initialize(self, _params: ()) -> (LanguageState, AnySchedulableResponse) {
        let SuperInit {
            client,
            exec_cmds,
            config,
            err,
        } = self;
        let const_config = config.const_config.clone();
        // Bootstrap server
        let service = LanguageState::main(client, config, err.is_none());

        if let Some(err) = err {
            return (service, Err(err));
        }

        // Respond to the host (LSP client)
        // Register these capabilities statically if the client does not support dynamic
        // registration
        let semantic_tokens_provider = match service.config.semantic_tokens {
            SemanticTokensMode::Enable if !const_config.tokens_dynamic_registration => {
                Some(get_semantic_tokens_options().into())
            }
            _ => None,
        };
        let document_formatting_provider = match service.config.formatter_mode {
            FormatterMode::Typstyle | FormatterMode::Typstfmt
                if !const_config.doc_fmt_dynamic_registration =>
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
                    commands: exec_cmds,
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
        (service, just_result(res))
    }
}

// region Configuration Items
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
// endregion Configuration Items

// todo: Config::default() doesn't initialize arguments from environment
// variables
/// The user configuration read from the editor.
#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Constant configuration for the server.
    pub const_config: ConstConfig,
    /// The compile configurations
    pub compile: CompileConfig,
    /// Dynamic configuration for semantic tokens.
    pub semantic_tokens: SemanticTokensMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter_mode: FormatterMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter_print_width: Option<u32>,
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
            .inspect(|v| self.formatter_mode = *v);
        try_(|| u32::deserialize(update.get("formatterPrintWidth")?).ok())
            .inspect(|v| self.formatter_print_width = Some(*v));
        self.compile.update_by_map(update)?;
        self.compile.validate()
    }

    /// Get the formatter configuration.
    pub fn formatter(&self) -> FormatUserConfig {
        FormatUserConfig {
            mode: self.formatter_mode,
            width: self.formatter_print_width.unwrap_or(120),
            position_encoding: self.const_config.position_encoding,
        }
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

impl Default for ConstConfig {
    fn default() -> Self {
        Self::from(&InitializeParams::default())
    }
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

/// The user configuration read from the editor.
#[derive(Debug, Default, Clone)]
pub struct CompileConfig {
    /// The workspace roots from initialization.
    pub roots: Vec<PathBuf>,
    /// The output directory for PDF export.
    pub output_path: PathPattern,
    /// The mode of PDF export.
    pub export_pdf: ExportMode,
    /// Specifies the root path of the project manually.
    pub root_path: Option<PathBuf>,
    /// Specifies the cli font options
    pub font_opts: CompileFontArgs,
    /// Whether to ignore system fonts
    pub system_fonts: Option<bool>,
    /// Specifies the font paths
    pub font_paths: Vec<PathBuf>,
    /// Computed fonts based on configuration.
    pub fonts: OnceCell<Derived<Deferred<Arc<FontResolverImpl>>>>,
    /// Notify the compile status to the editor.
    pub notify_status: bool,
    /// Enable periscope document in hover.
    pub periscope_args: Option<PeriscopeArgs>,
    /// Typst extra arguments.
    pub typst_extra_args: Option<CompileExtraOpts>,
    /// The preferred theme for the document.
    pub preferred_theme: Option<String>,
    /// Whether the configuration can have a default entry path.
    pub has_default_entry_path: bool,
    /// The inputs for the language server protocol.
    pub lsp_inputs: ImmutDict,
}

impl CompileConfig {
    /// Updates the configuration with a JSON object.
    pub fn update(&mut self, update: &JsonValue) -> anyhow::Result<()> {
        if let JsonValue::Object(update) = update {
            self.update_by_map(update)
        } else {
            bail!("got invalid configuration object {update}")
        }
    }

    /// Updates the configuration with a map.
    pub fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> anyhow::Result<()> {
        self.output_path =
            try_or_default(|| PathPattern::deserialize(update.get("outputPath")?).ok());
        self.export_pdf = try_or_default(|| ExportMode::deserialize(update.get("exportPdf")?).ok());
        self.root_path = try_(|| Some(update.get("rootPath")?.as_str()?.into()));
        self.notify_status = match try_(|| update.get("compileStatus")?.as_str()) {
            Some("enable") => true,
            Some("disable") | None => false,
            _ => bail!("compileStatus must be either 'enable' or 'disable'"),
        };
        self.preferred_theme = try_(|| Some(update.get("preferredTheme")?.as_str()?.to_owned()));
        log::info!("preferred theme: {:?} {:?}", self.preferred_theme, update);

        // periscope_args
        self.periscope_args = match update.get("hoverPeriscope") {
            Some(serde_json::Value::String(e)) if e == "enable" => Some(PeriscopeArgs::default()),
            Some(serde_json::Value::Null | serde_json::Value::String(..)) | None => None,
            Some(periscope_args) => match serde_json::from_value(periscope_args.clone()) {
                Ok(e) => Some(e),
                Err(e) => bail!("failed to parse hoverPeriscope: {e}"),
            },
        };
        if let Some(args) = self.periscope_args.as_mut() {
            if args.invert_color == "auto" && self.preferred_theme.as_deref() == Some("dark") {
                "always".clone_into(&mut args.invert_color);
            }
        }

        {
            let typst_args: Vec<String> = match update
                .get("typstExtraArgs")
                .cloned()
                .map(serde_json::from_value)
            {
                Some(Ok(e)) => e,
                Some(Err(e)) => bail!("failed to parse typstExtraArgs: {e}"),
                // Even if the list is none, it should be parsed since we have env vars to retrieve.
                None => Vec::new(),
            };

            let command = match CompileOnceArgs::try_parse_from(
                Some("typst-cli".to_owned()).into_iter().chain(typst_args),
            ) {
                Ok(e) => e,
                Err(e) => bail!("failed to parse typstExtraArgs: {e}"),
            };

            // Convert the input pairs to a dictionary.
            let inputs: TypstDict = if command.inputs.is_empty() {
                TypstDict::default()
            } else {
                let pairs = command.inputs.iter();
                let pairs = pairs.map(|(k, v)| (k.as_str().into(), v.as_str().into_value()));
                pairs.collect()
            };

            // todo: the command.root may be not absolute
            self.typst_extra_args = Some(CompileExtraOpts {
                entry: command.input.map(|e| Path::new(&e).into()),
                root_dir: command.root,
                inputs: Arc::new(Prehashed::new(inputs)),
                font: command.font,
                creation_timestamp: command.creation_timestamp,
            });
        }

        self.font_paths = try_or_default(|| Vec::<_>::deserialize(update.get("fontPaths")?).ok());
        self.system_fonts = try_(|| update.get("systemFonts")?.as_bool());

        self.has_default_entry_path = self.determine_default_entry_path().is_some();
        self.lsp_inputs = {
            let mut dict = TypstDict::default();

            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct PreviewInputs {
                pub version: u32,
                pub theme: String,
            }

            dict.insert(
                "x-preview".into(),
                serde_json::to_string(&PreviewInputs {
                    version: 1,
                    theme: self.preferred_theme.clone().unwrap_or_default(),
                })
                .unwrap()
                .into_value(),
            );

            Arc::new(Prehashed::new(dict))
        };

        self.validate()
    }

    /// Determines the root directory for the entry file.
    fn determine_root(&self, entry: Option<&ImmutPath>) -> Option<ImmutPath> {
        if let Some(path) = &self.root_path {
            return Some(path.as_path().into());
        }

        if let Some(root) = try_(|| self.typst_extra_args.as_ref()?.root_dir.as_ref()) {
            return Some(root.as_path().into());
        }

        if let Some(entry) = entry {
            for root in self.roots.iter() {
                if entry.starts_with(root) {
                    return Some(root.as_path().into());
                }
            }

            if !self.roots.is_empty() {
                log::warn!("entry is not in any set root directory");
            }

            if let Some(parent) = entry.parent() {
                return Some(parent.into());
            }
        }

        if !self.roots.is_empty() {
            return Some(self.roots[0].as_path().into());
        }

        None
    }

    /// Determines the default entry path.
    pub fn determine_default_entry_path(&self) -> Option<ImmutPath> {
        let extras = self.typst_extra_args.as_ref()?;
        // todo: pre-compute this when updating config
        if let Some(entry) = &extras.entry {
            if entry.is_relative() {
                let root = self.determine_root(None)?;
                return Some(root.join(entry).as_path().into());
            }
        }
        extras.entry.clone()
    }

    /// Determines the entry state.
    pub fn determine_entry(&self, entry: Option<ImmutPath>) -> EntryState {
        // todo: formalize untitled path
        // let is_untitled = entry.as_ref().is_some_and(|p| p.starts_with("/untitled"));
        // let root_dir = self.determine_root(if is_untitled { None } else {
        // entry.as_ref() });
        let root_dir = self.determine_root(entry.as_ref());

        let entry = match (entry, root_dir) {
            // (Some(entry), Some(root)) if is_untitled => Some(EntryState::new_rooted(
            //     root,
            //     Some(FileId::new(None, VirtualPath::new(entry))),
            // )),
            (Some(entry), Some(root)) => match entry.strip_prefix(&root) {
                Ok(stripped) => Some(EntryState::new_rooted(
                    root,
                    Some(FileId::new(None, VirtualPath::new(stripped))),
                )),
                Err(err) => {
                    log::info!("Entry is not in root directory: err {err:?}: entry: {entry:?}, root: {root:?}");
                    EntryState::new_rootless(entry)
                }
            },
            (Some(entry), None) => EntryState::new_rootless(entry),
            (None, Some(root)) => Some(EntryState::new_workspace(root)),
            (None, None) => None,
        };

        entry.unwrap_or_else(|| match self.determine_root(None) {
            Some(root) => EntryState::new_workspace(root),
            None => EntryState::new_detached(),
        })
    }

    /// Determines the font resolver.
    pub fn determine_fonts(&self) -> Deferred<Arc<FontResolverImpl>> {
        // todo: on font resolving failure, downgrade to a fake font book
        let font = || {
            let mut opts = self.font_opts.clone();

            if let Some(system_fonts) = self.system_fonts.or_else(|| {
                self.typst_extra_args
                    .as_ref()
                    .map(|x| x.font.ignore_system_fonts)
            }) {
                opts.ignore_system_fonts = !system_fonts;
            }

            let font_paths = (!self.font_paths.is_empty()).then_some(&self.font_paths);
            let font_paths =
                font_paths.or_else(|| self.typst_extra_args.as_ref().map(|x| &x.font.font_paths));
            if let Some(paths) = font_paths {
                opts.font_paths.clone_from(paths);
            }

            let root = OnceCell::new();
            for path in opts.font_paths.iter_mut() {
                if path.is_relative() {
                    if let Some(root) = root.get_or_init(|| self.determine_root(None)) {
                        let p = std::mem::take(path);
                        *path = root.join(p);
                    }
                }
            }

            log::info!("creating SharedFontResolver with {opts:?}");
            Derived(Deferred::new(|| {
                crate::world::LspUniverseBuilder::resolve_fonts(opts)
                    .map(Arc::new)
                    .expect("failed to create font book")
            }))
        };
        self.fonts.get_or_init(font).clone().0
    }

    /// Determines the `sys.inputs` for the entry file.
    pub fn determine_inputs(&self) -> ImmutDict {
        #[comemo::memoize]
        fn combine(lhs: ImmutDict, rhs: ImmutDict) -> ImmutDict {
            let mut dict = (**lhs).clone();
            for (k, v) in rhs.iter() {
                dict.insert(k.clone(), v.clone());
            }

            Arc::new(Prehashed::new(dict))
        }

        let user_inputs = self.determine_user_inputs();

        combine(user_inputs, self.lsp_inputs.clone())
    }

    /// Determines the creation timestamp.
    pub fn determine_creation_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.typst_extra_args.as_ref()?.creation_timestamp
    }

    fn determine_user_inputs(&self) -> ImmutDict {
        static EMPTY: Lazy<ImmutDict> = Lazy::new(ImmutDict::default);

        if let Some(extras) = &self.typst_extra_args {
            return extras.inputs.clone();
        }

        EMPTY.clone()
    }

    /// Applies the primary options related to compilation.
    #[allow(clippy::type_complexity)]
    pub fn primary_opts(
        &self,
    ) -> (
        Option<bool>,
        &Vec<PathBuf>,
        Option<&CompileFontArgs>,
        Option<Arc<Path>>,
    ) {
        (
            self.system_fonts,
            &self.font_paths,
            self.typst_extra_args.as_ref().map(|e| &e.font),
            self.determine_root(self.determine_default_entry_path().as_ref()),
        )
    }

    /// Validates the configuration.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(root) = &self.root_path {
            if !root.is_absolute() {
                bail!("rootPath must be an absolute path: {root:?}");
            }
        }

        if let Some(extra_args) = &self.typst_extra_args {
            if let Some(root) = &extra_args.root_dir {
                if !root.is_absolute() {
                    bail!("typstExtraArgs.root must be an absolute path: {root:?}");
                }
            }
        }

        Ok(())
    }
}

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
    /// Never export.
    #[default]
    Never,
    /// Export on saving the document, i.e. on `textDocument/didSave` events.
    OnSave,
    /// Export on typing, i.e. on `textDocument/didChange` events.
    OnType,
    /// Export when a document has a title and on saved, which is useful to
    /// filter out template files.
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

/// Additional options for compilation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompileExtraOpts {
    /// The root directory for compilation routine.
    pub root_dir: Option<PathBuf>,
    /// Path to entry
    pub entry: Option<ImmutPath>,
    /// Additional input arguments to compile the entry file.
    pub inputs: ImmutDict,
    /// Additional font paths.
    pub font: CompileFontArgs,
    /// The creation timestamp for various output.
    pub creation_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// The path pattern that could be substituted.
///
/// # Examples
/// - `$root` is the root of the project.
/// - `$root/$dir` is the parent directory of the input (main) file.
/// - `$root/main` will help store pdf file to `$root/main.pdf` constantly.
/// - (default) `$root/$dir/$name` will help store pdf file along with the input
///   file.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PathPattern(pub String);

impl PathPattern {
    /// Creates a new path pattern.
    pub fn new(pattern: &str) -> Self {
        Self(pattern.to_owned())
    }

    /// Substitutes the path pattern with `$root`, and `$dir/$name`.
    pub fn substitute(&self, entry: &EntryState) -> Option<ImmutPath> {
        self.substitute_impl(entry.root(), entry.main())
    }

    #[comemo::memoize]
    fn substitute_impl(&self, root: Option<ImmutPath>, main: Option<FileId>) -> Option<ImmutPath> {
        log::info!("Check path {main:?} and root {root:?} with output directory {self:?}");

        let (root, main) = root.zip(main)?;

        // Files in packages are not exported
        if main.package().is_some() {
            return None;
        }
        // Files without a path are not exported
        let path = main.vpath().resolve(&root)?;

        // todo: handle untitled path
        if let Ok(path) = path.strip_prefix("/untitled") {
            let tmp = std::env::temp_dir();
            let path = tmp.join("typst").join(path);
            return Some(path.as_path().into());
        }

        if self.0.is_empty() {
            return Some(path.to_path_buf().clean().into());
        }

        let path = path.strip_prefix(&root).ok()?;
        let dir = path.parent();
        let file_name = path.file_name().unwrap_or_default();

        let w = root.to_string_lossy();
        let f = file_name.to_string_lossy();

        // replace all $root
        let mut path = self.0.replace("$root", &w);
        if let Some(dir) = dir {
            let d = dir.to_string_lossy();
            path = path.replace("$dir", &d);
        }
        path = path.replace("$name", &f);

        Some(PathBuf::from(path).clean().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_default_encoding() {
        let cc = ConstConfig::default();
        assert_eq!(cc.position_encoding, PositionEncoding::Utf16);
    }

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

        // Nix specifies this environment variable when testing.
        let has_source_date_epoch = std::env::var("SOURCE_DATE_EPOCH").is_ok();
        if has_source_date_epoch {
            let args = config.compile.typst_extra_args.as_mut().unwrap();
            assert!(args.creation_timestamp.is_some());
            args.creation_timestamp = None;
        }

        assert_eq!(config.compile.output_path, PathPattern::new("out"));
        assert_eq!(config.compile.export_pdf, ExportMode::OnSave);
        assert_eq!(config.compile.root_path, Some(PathBuf::from(root_path)));
        assert_eq!(config.semantic_tokens, SemanticTokensMode::Enable);
        assert_eq!(config.formatter_mode, FormatterMode::Typstyle);
        assert_eq!(
            config.compile.typst_extra_args,
            Some(CompileExtraOpts {
                root_dir: Some(PathBuf::from(root_path)),
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_config_creation_timestamp() {
        type Timestamp = Option<chrono::DateTime<chrono::Utc>>;

        fn timestamp(f: impl FnOnce(&mut Config)) -> Timestamp {
            let mut config = Config::default();

            f(&mut config);

            let args = config.compile.typst_extra_args;
            args.and_then(|args| args.creation_timestamp)
        }

        // assert!(timestamp(|_| {}).is_none());
        // assert!(timestamp(|config| {
        //     let update = json!({});
        //     config.update(&update).unwrap();
        // })
        // .is_none());

        let args_timestamp = timestamp(|config| {
            let update = json!({
                "typstExtraArgs": ["--creation-timestamp", "1234"]
            });
            config.update(&update).unwrap();
        });
        assert!(args_timestamp.is_some());

        // todo: concurrent get/set env vars is unsafe
        //     std::env::set_var("SOURCE_DATE_EPOCH", "1234");
        //     let env_timestamp = timestamp(|config| {
        //         config.update(&json!({})).unwrap();
        //     });

        //     assert_eq!(args_timestamp, env_timestamp);
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
        assert!(err.contains("absolute path"), "unexpected error: {err}");
    }

    #[test]
    fn test_reject_abnormal_root2() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": ["--root", "."]
        });

        let err = format!("{}", config.update(&update).unwrap_err());
        assert!(err.contains("absolute path"), "unexpected error: {err}");
    }

    #[test]
    fn test_substitute_path() {
        let root = Path::new("/root");
        let entry = EntryState::new_rooted(
            root.into(),
            Some(FileId::new(None, VirtualPath::new("/dir1/dir2/file.txt"))),
        );

        assert_eq!(
            PathPattern::new("/substitute/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/dir2/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$dir/../$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/target/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/target/dir1/dir2/file.txt").into())
        );
    }

    #[test]
    fn test_default_formatting_config() {
        let config = Config::default().formatter();
        assert_eq!(config.mode, FormatterMode::Disable);
        assert_eq!(config.width, 120);
        assert_eq!(config.position_encoding, PositionEncoding::Utf16);
    }
}
