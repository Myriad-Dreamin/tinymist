use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::project::font::TinymistFontResolver;
use crate::world::EntryState;
use anyhow::bail;
use clap::Parser;
use itertools::Itertools;
use lsp_types::*;
use once_cell::sync::{Lazy, OnceCell};
use reflexo::path::PathClean;
use reflexo_typst::{ImmutPath, TypstDict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use strum::IntoEnumIterator;
use task::{FormatUserConfig, FormatterConfig};
use tinymist_query::analysis::{Modifier, TokenType};
use tinymist_query::{CompletionFeat, EntryResolver, PositionEncoding};
use tinymist_render::PeriscopeArgs;
use typst::foundations::IntoValue;
use typst::syntax::FileId;
use typst_shim::utils::{Deferred, LazyHash};
use vfs::WorkspaceResolver;

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
        let roots = match params.workspace_folders.as_ref() {
            Some(roots) => roots
                .iter()
                .filter_map(|root| root.uri.to_file_path().ok().map(ImmutPath::from))
                .collect(),
            #[allow(deprecated)] // `params.root_path` is marked as deprecated
            None => params
                .root_uri
                .as_ref()
                .and_then(|uri| uri.to_file_path().ok().map(ImmutPath::from))
                .or_else(|| Some(Path::new(&params.root_path.as_ref()?).into()))
                .into_iter()
                .collect(),
        };
        let mut config = Config {
            const_config: ConstConfig::from(&params),
            compile: CompileConfig {
                entry_resolver: EntryResolver {
                    roots,
                    ..Default::default()
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

        let semantic_tokens_provider = (!const_config.tokens_dynamic_registration).then(|| {
            SemanticTokensServerCapabilities::SemanticTokensOptions(get_semantic_tokens_options())
        });
        let document_formatting_provider =
            (!const_config.doc_fmt_dynamic_registration).then_some(OneOf::Left(true));

        let file_operations = const_config.notify_will_rename_files.then(|| {
            WorkspaceFileOperationsServerCapabilities {
                will_rename: Some(FileOperationRegistrationOptions {
                    filters: vec![FileOperationFilter {
                        scheme: Some("file".to_string()),
                        pattern: FileOperationPattern {
                            glob: "**/*.typ".to_string(),
                            matches: Some(FileOperationPatternKind::File),
                            options: None,
                        },
                    }],
                }),
                ..Default::default()
            }
        });

        let res = InitializeResult {
            capabilities: ServerCapabilities {
                // todo: respect position_encoding
                // position_encoding: Some(cc.position_encoding.into()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![
                        String::from("("),
                        String::from(","),
                        String::from(":"),
                    ]),
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
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations,
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
    "tinymist",
    "outputPath",
    "exportPdf",
    "rootPath",
    "semanticTokens",
    "formatterMode",
    "formatterPrintWidth",
    "completion",
    "fontPaths",
    "systemFonts",
    "typstExtraArgs",
    "compileStatus",
    "colorTheme",
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
    /// Whether to remove html from markup content in responses.
    pub support_html_in_markdown: bool,
    /// Tinymist's completion features.
    pub completion: CompletionFeat,
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
            let namespaced = update.get("tinymist").and_then(|m| match m {
                JsonValue::Object(namespaced) => Some(namespaced),
                _ => None,
            });

            self.update_by_map(update)?;
            if let Some(namespaced) = namespaced {
                self.update_by_map(namespaced)?;
            }
            Ok(())
        } else {
            bail!("got invalid configuration object {update}")
        }
    }

    /// Updates the configuration with a map.
    ///
    /// # Errors
    /// Errors if the update is invalid.
    pub fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> anyhow::Result<()> {
        macro_rules! assign_config {
            ($( $field_path:ident ).+ := $bind:literal?: $ty:ty) => {
                let v = try_deserialize::<$ty>(update, $bind);
                self.$($field_path).+ = v.unwrap_or_default();
            };
            ($( $field_path:ident ).+ := $bind:literal: $ty:ty = $default_value:expr) => {
                let v = try_deserialize::<$ty>(update, $bind);
                self.$($field_path).+ = v.unwrap_or_else(|| $default_value);
            };
        }

        fn try_deserialize<T: serde::de::DeserializeOwned>(
            map: &Map<String, JsonValue>,
            key: &str,
        ) -> Option<T> {
            T::deserialize(map.get(key)?)
                .inspect_err(|e| log::warn!("failed to deserialize {key:?}: {e}"))
                .ok()
        }

        assign_config!(semantic_tokens := "semanticTokens"?: SemanticTokensMode);
        assign_config!(formatter_mode := "formatterMode"?: FormatterMode);
        assign_config!(formatter_print_width := "formatterPrintWidth"?: Option<u32>);
        assign_config!(support_html_in_markdown := "supportHtmlInMarkdown"?: bool);
        assign_config!(completion := "completion"?: CompletionFeat);
        assign_config!(completion.trigger_suggest := "triggerSuggest"?: bool);
        assign_config!(completion.trigger_parameter_hints := "triggerParameterHints"?: bool);
        assign_config!(completion.trigger_suggest_and_parameter_hints := "triggerSuggestAndParameterHints"?: bool);
        self.compile.update_by_map(update)?;
        self.compile.validate()
    }

    /// Get the formatter configuration.
    pub fn formatter(&self) -> FormatUserConfig {
        let formatter_print_width = self.formatter_print_width.unwrap_or(120) as usize;

        FormatUserConfig {
            config: match self.formatter_mode {
                FormatterMode::Typstyle => FormatterConfig::Typstyle(Box::new(
                    typstyle_core::PrinterConfig::new_with_width(formatter_print_width),
                )),
                FormatterMode::Typstfmt => FormatterConfig::Typstfmt(Box::new(typstfmt::Config {
                    max_line_length: formatter_print_width,
                    ..typstfmt::Config::default()
                })),
                FormatterMode::Disable => FormatterConfig::Disable,
            },
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
    /// Allow notifying workspace/didRenameFiles
    pub notify_will_rename_files: bool,
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
        let file_operations = try_(|| workspace?.file_operations.as_ref());
        let doc = params.capabilities.text_document.as_ref();
        let sema = try_(|| doc?.semantic_tokens.as_ref());
        let fold = try_(|| doc?.folding_range.as_ref());
        let format = try_(|| doc?.formatting.as_ref());

        Self {
            position_encoding,
            cfg_change_registration: try_or(|| workspace?.configuration, false),
            notify_will_rename_files: try_or(|| file_operations?.will_rename, false),
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
    /// The output directory for PDF export.
    pub output_path: PathPattern,
    /// The mode of PDF export.
    pub export_pdf: ExportMode,
    /// Specifies the cli font options
    pub font_opts: CompileFontArgs,
    /// Whether to ignore system fonts
    pub system_fonts: Option<bool>,
    /// Specifies the font paths
    pub font_paths: Vec<PathBuf>,
    /// Computed fonts based on configuration.
    pub fonts: OnceCell<Derived<Deferred<Arc<TinymistFontResolver>>>>,
    /// Notify the compile status to the editor.
    pub notify_status: bool,
    /// Enable periscope document in hover.
    pub periscope_args: Option<PeriscopeArgs>,
    /// Typst extra arguments.
    pub typst_extra_args: Option<CompileExtraOpts>,
    /// The preferred color theme for the document.
    pub color_theme: Option<String>,
    /// Whether the configuration can have a default entry path.
    pub has_default_entry_path: bool,
    /// The inputs for the language server protocol.
    pub lsp_inputs: ImmutDict,
    /// The entry resolver.
    pub entry_resolver: EntryResolver,
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
        macro_rules! deser_or_default {
            ($key:expr, $ty:ty) => {
                try_or_default(|| <$ty>::deserialize(update.get($key)?).ok())
            };
        }

        self.output_path = deser_or_default!("outputPath", PathPattern);
        self.export_pdf = deser_or_default!("exportPdf", ExportMode);
        self.notify_status = match try_(|| update.get("compileStatus")?.as_str()) {
            Some("enable") => true,
            Some("disable") | None => false,
            _ => bail!("compileStatus must be either 'enable' or 'disable'"),
        };
        self.color_theme = try_(|| Some(update.get("colorTheme")?.as_str()?.to_owned()));
        log::info!("color theme: {:?}", self.color_theme);

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
            if args.invert_color == "auto" && self.color_theme.as_deref() == Some("dark") {
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
                root_dir: command.root.as_ref().map(|r| r.as_path().into()),
                inputs: Arc::new(LazyHash::new(inputs)),
                font: command.font,
                package: command.package,
                creation_timestamp: command.creation_timestamp,
                cert: command.cert.as_deref().map(From::from),
            });
        }

        self.font_paths = try_or_default(|| Vec::<_>::deserialize(update.get("fontPaths")?).ok());
        self.system_fonts = try_(|| update.get("systemFonts")?.as_bool());

        self.entry_resolver.root_path =
            try_(|| Some(Path::new(update.get("rootPath")?.as_str()?).into())).or_else(|| {
                self.typst_extra_args
                    .as_ref()
                    .and_then(|e| e.root_dir.clone())
            });
        self.entry_resolver.entry = self.typst_extra_args.as_ref().and_then(|e| e.entry.clone());
        self.has_default_entry_path = self.entry_resolver.resolve_default().is_some();
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
                    theme: self.color_theme.clone().unwrap_or_default(),
                })
                .unwrap()
                .into_value(),
            );

            Arc::new(LazyHash::new(dict))
        };

        self.validate()
    }

    /// Determines the font options.
    pub fn determine_font_opts(&self) -> CompileFontArgs {
        let mut opts = self.font_opts.clone();

        if let Some(system_fonts) = self.system_fonts.or_else(|| {
            self.typst_extra_args
                .as_ref()
                .map(|x| !x.font.ignore_system_fonts)
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
                if let Some(root) = root.get_or_init(|| self.entry_resolver.root(None)) {
                    let p = std::mem::take(path);
                    *path = root.join(p);
                }
            }
        }

        opts
    }

    /// Determines the package options.
    pub fn determine_package_opts(&self) -> CompilePackageArgs {
        if let Some(extras) = &self.typst_extra_args {
            return extras.package.clone();
        }
        CompilePackageArgs::default()
    }

    /// Determines the font resolver.
    pub fn determine_fonts(&self) -> Deferred<Arc<TinymistFontResolver>> {
        // todo: on font resolving failure, downgrade to a fake font book
        let font = || {
            let opts = self.determine_font_opts();

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

            Arc::new(LazyHash::new(dict))
        }

        let user_inputs = self.determine_user_inputs();

        combine(user_inputs, self.lsp_inputs.clone())
    }

    /// Determines the creation timestamp.
    pub fn determine_creation_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.typst_extra_args.as_ref()?.creation_timestamp
    }

    /// Determines the certification path.
    pub fn determine_certification_path(&self) -> Option<ImmutPath> {
        let extras = self.typst_extra_args.as_ref()?;
        extras.cert.clone()
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
            self.entry_resolver
                .root(self.entry_resolver.resolve_default().as_ref()),
        )
    }

    /// Validates the configuration.
    pub fn validate(&self) -> anyhow::Result<()> {
        self.entry_resolver.validate()?;

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

pub(crate) fn get_semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: SemanticTokensLegend {
            token_types: TokenType::iter()
                .filter(|e| *e != TokenType::None)
                .map(Into::into)
                .collect(),
            token_modifiers: Modifier::iter().map(Into::into).collect(),
        },
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
        ..Default::default()
    }
}

/// Additional options for compilation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompileExtraOpts {
    /// The root directory for compilation routine.
    pub root_dir: Option<ImmutPath>,
    /// Path to entry
    pub entry: Option<ImmutPath>,
    /// Additional input arguments to compile the entry file.
    pub inputs: ImmutDict,
    /// Additional font paths.
    pub font: CompileFontArgs,
    /// Package related arguments.
    pub package: CompilePackageArgs,
    /// The creation timestamp for various output.
    pub creation_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Path to certification file
    pub cert: Option<ImmutPath>,
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
        if WorkspaceResolver::is_package_file(main) {
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
    use typst::syntax::VirtualPath;

    #[test]
    fn test_default_encoding() {
        let cc = ConstConfig::default();
        assert_eq!(cc.position_encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn test_config_update() {
        let mut config = Config::default();

        let root_path = Path::new(if cfg!(windows) { "C:\\root" } else { "/root" });

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
        assert_eq!(
            config.compile.entry_resolver.root_path,
            Some(ImmutPath::from(root_path))
        );
        assert_eq!(config.semantic_tokens, SemanticTokensMode::Enable);
        assert_eq!(config.formatter_mode, FormatterMode::Typstyle);
        assert_eq!(
            config.compile.typst_extra_args,
            Some(CompileExtraOpts {
                root_dir: Some(ImmutPath::from(root_path)),
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_namespaced_config() {
        let mut config = Config::default();

        // Emacs uses a shared configuration object for all language servers.
        let update = json!({
            "exportPdf": "onSave",
            "tinymist": {
                "exportPdf": "onType",
            }
        });

        config.update(&update).unwrap();

        assert_eq!(config.compile.export_pdf, ExportMode::OnType);
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
    fn test_font_opts() {
        fn opts(update: Option<&JsonValue>) -> CompileFontArgs {
            let mut config = Config::default();
            if let Some(update) = update {
                config.update(update).unwrap();
            }

            config.compile.determine_font_opts()
        }

        let font_opts = opts(None);
        assert!(!font_opts.ignore_system_fonts);

        let font_opts = opts(Some(&json!({})));
        assert!(!font_opts.ignore_system_fonts);

        let font_opts = opts(Some(&json!({
            "typstExtraArgs": []
        })));
        assert!(!font_opts.ignore_system_fonts);

        let font_opts = opts(Some(&json!({
            "systemFonts": false,
        })));
        assert!(font_opts.ignore_system_fonts);

        let font_opts = opts(Some(&json!({
            "typstExtraArgs": ["--ignore-system-fonts"]
        })));
        assert!(font_opts.ignore_system_fonts);

        let font_opts = opts(Some(&json!({
            "systemFonts": true,
            "typstExtraArgs": ["--ignore-system-fonts"]
        })));
        assert!(!font_opts.ignore_system_fonts);
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
    fn test_entry_by_extra_args() {
        let simple_config = {
            let mut config = Config::default();
            let update = json!({
                "typstExtraArgs": ["main.typ"]
            });

            // It should be able to resolve the entry file from the extra arguments.
            config.update(&update).expect("updated");
            // Passing it twice doesn't affect the result.
            config.update(&update).expect("updated");
            config
        };
        {
            let mut config = Config::default();
            let update = json!({
                "typstExtraArgs": ["main.typ", "main.typ"]
            });

            let err = format!("{}", config.update(&update).unwrap_err());
            assert!(
                err.contains("unexpected argument"),
                "unexpected error: {err}"
            );
            assert!(err.contains("help"), "unexpected error: {err}");
        }
        {
            let mut config = Config::default();
            let update = json!({
                "typstExtraArgs": ["main2.typ"],
                "tinymist": {
                    "typstExtraArgs": ["main.typ"]
                }
            });

            // It should be able to resolve the entry file from the extra arguments.
            config.update(&update).expect("updated");
            // Passing it twice doesn't affect the result.
            config.update(&update).expect("updated");

            assert_eq!(
                config.compile.typst_extra_args,
                simple_config.compile.typst_extra_args
            );
        }
    }

    #[test]
    fn test_substitute_path() {
        let root = Path::new("/root");
        let entry =
            EntryState::new_rooted(root.into(), Some(VirtualPath::new("/dir1/dir2/file.txt")));

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
        assert!(matches!(config.config, FormatterConfig::Disable));
        assert_eq!(config.position_encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn test_typstyle_formatting_config() {
        let config = Config {
            formatter_mode: FormatterMode::Typstyle,
            ..Config::default()
        };
        let config = config.formatter();
        assert_eq!(config.position_encoding, PositionEncoding::Utf16);

        let typstyle_config = match config.config {
            FormatterConfig::Typstyle(e) => e,
            _ => panic!("unexpected configuration of formatter"),
        };

        assert_eq!(typstyle_config.max_width, 120);
    }

    #[test]
    fn test_typstyle_formatting_config_set_width() {
        let config = Config {
            formatter_mode: FormatterMode::Typstyle,
            formatter_print_width: Some(240),
            ..Config::default()
        };
        let config = config.formatter();
        assert_eq!(config.position_encoding, PositionEncoding::Utf16);

        let typstyle_config = match config.config {
            FormatterConfig::Typstyle(e) => e,
            _ => panic!("unexpected configuration of formatter"),
        };

        assert_eq!(typstyle_config.max_width, 240);
    }
}
