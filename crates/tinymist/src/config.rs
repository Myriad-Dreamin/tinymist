use core::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use itertools::Itertools;
use lsp_types::*;
use once_cell::sync::{Lazy, OnceCell};
use reflexo::error::IgnoreLogging;
use reflexo::CowStr;
use reflexo_typst::{ImmutPath, TypstDict};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use strum::IntoEnumIterator;
use task::{ExportUserConfig, FormatUserConfig, FormatterConfig};
use tinymist_l10n::DebugL10n;
use tinymist_query::analysis::{Modifier, TokenType};
use tinymist_query::{CompletionFeat, PositionEncoding};
use tinymist_render::PeriscopeArgs;
use tinymist_std::error::prelude::*;
use tinymist_task::ExportTarget;
use typst::foundations::IntoValue;
use typst_shim::utils::LazyHash;

use super::*;
use crate::project::font::TinymistFontResolver;
use crate::project::{
    EntryResolver, ExportPdfTask, ExportTask, ImmutDict, PathPattern, ProjectResolutionKind,
    ProjectTask, TaskWhen,
};

// region Configuration Items
const CONFIG_ITEMS: &[&str] = &[
    "tinymist",
    "colorTheme",
    "compileStatus",
    "completion",
    "exportPdf",
    "exportTarget",
    "fontPaths",
    "formatterMode",
    "formatterPrintWidth",
    "formatterIndentSize",
    "hoverPeriscope",
    "outputPath",
    "preview",
    "projectResolution",
    "rootPath",
    "semanticTokens",
    "systemFonts",
    "typstExtraArgs",
];
// endregion Configuration Items

/// The user configuration read from the editor.
///
/// Note: `Config::default` is intentionally to be "pure" and not to be
/// affected by system environment variables.
/// To get the configuration with system defaults, use [`Config::new`] instead.
#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Constant configuration during session.
    pub const_config: ConstConfig,
    /// Constant DAP-specific configuration during session.
    pub const_dap_config: ConstDapConfig,

    /// Whether to send show document requests with customized notification.
    pub customized_show_document: bool,
    /// Whether the configuration can have a default entry path.
    pub has_default_entry_path: bool,
    /// Whether to notify the compile status to the editor.
    pub notify_status: bool,
    /// Whether to remove HTML from markup content in responses.
    pub support_html_in_markdown: bool,

    /// The preferred color theme for rendering.
    pub color_theme: Option<String>,
    /// The entry resolver.
    pub entry_resolver: EntryResolver,
    /// The `sys.inputs` passed to the typst compiler.
    pub lsp_inputs: ImmutDict,
    /// The arguments about periscope rendering in hover window.
    pub periscope_args: Option<PeriscopeArgs>,
    /// The extra typst arguments passed to the language server.
    pub typst_extra_args: Option<TypstExtraArgs>,
    /// The dynamic configuration for semantic tokens.
    pub semantic_tokens: SemanticTokensMode,

    /// Tinymist's completion features.
    pub completion: CompletionFeat,
    /// Tinymist's preview features.
    pub preview: PreviewFeat,

    /// Specifies the cli font options
    pub font_opts: CompileFontArgs,
    /// Specifies the font paths
    pub font_paths: Vec<PathBuf>,
    /// Computed fonts based on configuration.
    pub fonts: OnceCell<Derived<Arc<TinymistFontResolver>>>,
    /// Whether to use system fonts.
    pub system_fonts: Option<bool>,

    /// Tinymist's default export target.
    pub export_target: ExportTarget,
    /// The mode of PDF export.
    pub export_pdf: TaskWhen,
    /// The output directory for PDF export.
    pub output_path: PathPattern,

    /// Dynamic configuration for the experimental formatter.
    pub formatter_mode: FormatterMode,
    /// Sets the print width for the formatter, which is a **soft limit** of
    /// characters per line. See [the definition of *Print Width*](https://prettier.io/docs/en/options.html#print-width).
    pub formatter_print_width: Option<u32>,
    /// Sets the indent size (using space) for the formatter.
    pub formatter_indent_size: Option<u32>,

    /// The warnings during configuration update.
    pub warnings: Vec<CowStr>,
}

impl Config {
    /// Creates a new configuration with system defaults.
    pub fn new(
        const_config: ConstConfig,
        roots: Vec<ImmutPath>,
        font_opts: CompileFontArgs,
    ) -> Self {
        let mut config = Self {
            const_config,
            const_dap_config: ConstDapConfig::default(),
            entry_resolver: EntryResolver {
                roots,
                ..EntryResolver::default()
            },
            font_opts,
            ..Self::default()
        };
        config
            .update_by_map(&Map::default())
            .log_error("failed to assign Config defaults");
        config
    }

    /// Creates a new configuration from the LSP initialization parameters.
    ///
    /// The function has side effects:
    /// - Getting environment variables.
    /// - Setting the locale.
    pub fn extract_lsp_params(
        params: InitializeParams,
        font_args: CompileFontArgs,
    ) -> (Self, Option<ResponseError>) {
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
        let mut config = Config::new(ConstConfig::from(&params), roots, font_args);

        // Sets locale as soon as possible
        if let Some(locale) = config.const_config.locale.as_ref() {
            tinymist_l10n::set_locale(locale);
        }

        let err = params
            .initialization_options
            .and_then(|init| config.update(&init).map_err(invalid_params).err());

        (config, err)
    }

    /// Creates a new configuration from the DAP initialization parameters.
    ///
    /// The function has side effects:
    /// - Getting environment variables.
    /// - Setting the locale.
    pub fn extract_dap_params(
        params: dapts::InitializeRequestArguments,
        font_args: CompileFontArgs,
    ) -> (Self, Option<ResponseError>) {
        // This is reliable in DAP context.
        let cwd = std::env::current_dir()
            .expect("failed to get current directory")
            .into();

        // Initialize configurations
        let roots = vec![cwd];
        let mut config = Config::new(ConstConfig::from(&params), roots, font_args);
        config.const_dap_config = ConstDapConfig::from(&params);

        // Sets locale as soon as possible
        if let Some(locale) = config.const_config.locale.as_ref() {
            tinymist_l10n::set_locale(locale);
        }

        (config, None)
    }

    /// Gets configuration descriptors to request configuration sections from
    /// the client.
    pub fn get_items() -> Vec<ConfigurationItem> {
        CONFIG_ITEMS
            .iter()
            .flat_map(|&item| [format!("tinymist.{item}"), item.to_owned()])
            .map(|section| ConfigurationItem {
                section: Some(section),
                ..ConfigurationItem::default()
            })
            .collect()
    }

    /// Converts config values to a map object.
    pub fn values_to_map(values: Vec<JsonValue>) -> Map<String, JsonValue> {
        let unpaired_values = values
            .into_iter()
            .tuples()
            .map(|(a, b)| if !a.is_null() { a } else { b });

        CONFIG_ITEMS
            .iter()
            .map(|&item| item.to_owned())
            .zip(unpaired_values)
            .collect()
    }

    /// Updates (and validates) the configuration by a JSON object.
    ///
    /// The config may be broken if the update is invalid. Please clone the
    /// configuration before updating and revert if the update fails.
    pub fn update(&mut self, update: &JsonValue) -> Result<()> {
        if let JsonValue::Object(update) = update {
            self.update_by_map(update)?;

            // Configurations in the tinymist namespace take precedence.
            if let Some(namespaced) = update.get("tinymist").and_then(JsonValue::as_object) {
                self.update_by_map(namespaced)?;
            }

            Ok(())
        } else {
            tinymist_l10n::bail!(
                "tinymist.config.invalidObject",
                "invalid configuration object: {object}",
                object = update.debug_l10n(),
            )
        }
    }

    /// Updates (and validates) the configuration by a map object.
    ///
    /// The config may be broken if the update is invalid. Please clone the
    /// configuration before updating and revert if the update fails.
    pub fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> Result<()> {
        log::info!(
            "ServerState: config update_by_map {}",
            serde_json::to_string(update).unwrap_or_else(|e| e.to_string())
        );

        self.warnings.clear();

        macro_rules! try_deserialize {
            ($ty:ty, $key:expr) => {
                update.get($key).and_then(|v| {
                    <$ty>::deserialize(v)
                        .inspect_err(|err| {
                            // Only ignore null returns. Some editors may send null values when
                            // the configuration is not set, e.g. Zed.
                            if v.is_null() {
                                return;
                            }

                            self.warnings.push(tinymist_l10n::t!(
                                "tinymist.config.deserializeError",
                                "failed to deserialize \"{key}\": {err}",
                                key = $key.debug_l10n(),
                                err = err.debug_l10n(),
                            ));
                        })
                        .ok()
                })
            };
        }

        macro_rules! assign_config {
            ($( $field_path:ident ).+ := $bind:literal?: $ty:ty) => {
                let v = try_deserialize!($ty, $bind);
                self.$($field_path).+ = v.unwrap_or_default();
            };
            ($( $field_path:ident ).+ := $bind:literal: $ty:ty = $default_value:expr) => {
                let v = try_deserialize!($ty, $bind);
                self.$($field_path).+ = v.unwrap_or_else(|| $default_value);
            };
        }

        assign_config!(color_theme := "colorTheme"?: Option<String>);
        assign_config!(completion := "completion"?: CompletionFeat);
        assign_config!(completion.trigger_suggest := "triggerSuggest"?: bool);
        assign_config!(completion.trigger_parameter_hints := "triggerParameterHints"?: bool);
        assign_config!(completion.trigger_suggest_and_parameter_hints := "triggerSuggestAndParameterHints"?: bool);
        assign_config!(customized_show_document := "customizedShowDocument"?: bool);
        assign_config!(entry_resolver.project_resolution := "projectResolution"?: ProjectResolutionKind);
        assign_config!(export_pdf := "exportPdf"?: TaskWhen);
        assign_config!(export_target := "exportTarget"?: ExportTarget);
        assign_config!(font_paths := "fontPaths"?: Vec<_>);
        assign_config!(formatter_mode := "formatterMode"?: FormatterMode);
        assign_config!(formatter_print_width := "formatterPrintWidth"?: Option<u32>);
        assign_config!(formatter_indent_size := "formatterIndentSize"?: Option<u32>);
        assign_config!(output_path := "outputPath"?: PathPattern);
        assign_config!(preview := "preview"?: PreviewFeat);
        assign_config!(semantic_tokens := "semanticTokens"?: SemanticTokensMode);
        assign_config!(support_html_in_markdown := "supportHtmlInMarkdown"?: bool);
        assign_config!(system_fonts := "systemFonts"?: Option<bool>);

        self.notify_status = match try_(|| update.get("compileStatus")?.as_str()) {
            Some("enable") => true,
            Some("disable") | None => false,
            Some(value) => {
                self.warnings.push(tinymist_l10n::t!(
                    "tinymist.config.badCompileStatus",
                    "compileStatus must be either `\"enable\"` or `\"disable\"`, got {value}",
                    value = value.debug_l10n(),
                ));

                false
            }
        };

        // periscope_args
        self.periscope_args = match update.get("hoverPeriscope") {
            Some(serde_json::Value::String(e)) if e == "enable" => Some(PeriscopeArgs::default()),
            Some(serde_json::Value::Null | serde_json::Value::String(..)) | None => None,
            Some(periscope_args) => match serde_json::from_value(periscope_args.clone()) {
                Ok(args) => Some(args),
                Err(err) => {
                    self.warnings.push(tinymist_l10n::t!(
                        "tinymist.config.badHoverPeriscope",
                        "failed to parse hoverPeriscope: {err}",
                        err = err.debug_l10n(),
                    ));
                    None
                }
            },
        };
        if let Some(args) = self.periscope_args.as_mut() {
            if args.invert_color == "auto" && self.color_theme.as_deref() == Some("dark") {
                "always".clone_into(&mut args.invert_color);
            }
        }

        fn invalid_extra_args(args: &impl fmt::Debug, err: impl std::error::Error) -> CowStr {
            log::warn!("failed to parse typstExtraArgs: {err}, args: {args:?}");
            tinymist_l10n::t!(
                "tinymist.config.badTypstExtraArgs",
                "failed to parse typstExtraArgs: {err}, args: {args}",
                err = err.debug_l10n(),
                args = args.debug_l10n(),
            )
        }

        {
            let raw_args = || update.get("typstExtraArgs");
            let typst_args: Vec<String> = match raw_args().cloned().map(serde_json::from_value) {
                Some(Ok(args)) => args,
                Some(Err(err)) => {
                    self.warnings.push(invalid_extra_args(&raw_args(), err));
                    None
                }
                // Even if the list is none, it should be parsed since we have env vars to
                // retrieve.
                None => None,
            }
            .unwrap_or_default();
            let empty_typst_args = typst_args.is_empty();

            let args = match CompileOnceArgs::try_parse_from(
                Some("typst-cli".to_owned()).into_iter().chain(typst_args),
            ) {
                Ok(args) => args,
                Err(err) => {
                    self.warnings.push(invalid_extra_args(&raw_args(), err));

                    if empty_typst_args {
                        CompileOnceArgs::default()
                    } else {
                        // Still try to parse the arguments to get the environment variables.
                        CompileOnceArgs::try_parse_from(Some("typst-cli".to_owned()))
                            .inspect_err(|err| {
                                log::error!("failed to make default typstExtraArgs: {err}");
                            })
                            .unwrap_or_default()
                    }
                }
            };

            // todo: the command.root may be not absolute
            self.typst_extra_args = Some(TypstExtraArgs {
                inputs: args.resolve_inputs().unwrap_or_default(),
                entry: args.input.map(|e| Path::new(&e).into()),
                root_dir: args.root.as_ref().map(|r| r.as_path().into()),
                font: args.font,
                package: args.package,
                creation_timestamp: args.creation_timestamp,
                cert: args.cert.as_deref().map(From::from),
            });
        }

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

    /// Validates the configuration.
    pub fn validate(&self) -> Result<()> {
        self.entry_resolver.validate()?;

        Ok(())
    }

    /// Gets the formatter configuration.
    pub fn formatter(&self) -> FormatUserConfig {
        let formatter_print_width = self.formatter_print_width.unwrap_or(120) as usize;
        let formatter_indent_size = self.formatter_indent_size.unwrap_or(2) as usize;

        FormatUserConfig {
            config: match self.formatter_mode {
                FormatterMode::Typstyle => FormatterConfig::Typstyle(Box::new(
                    typstyle_core::Config::default()
                        .with_width(formatter_print_width)
                        .with_tab_spaces(formatter_indent_size),
                )),
                FormatterMode::Typstfmt => FormatterConfig::Typstfmt(Box::new(typstfmt::Config {
                    max_line_length: formatter_print_width,
                    indent_space: formatter_indent_size,
                    ..typstfmt::Config::default()
                })),
                FormatterMode::Disable => FormatterConfig::Disable,
            },
            position_encoding: self.const_config.position_encoding,
        }
    }

    /// Gets the export task configuration.
    pub(crate) fn export_task(&self) -> ExportTask {
        ExportTask {
            when: self.export_pdf,
            output: Some(self.output_path.clone()),
            transform: vec![],
        }
    }

    /// Gets the export configuration.
    pub(crate) fn export(&self) -> ExportUserConfig {
        let export = self.export_task();
        ExportUserConfig {
            export_target: self.export_target,
            // todo: we only have `exportPdf` for now
            // task: match self.export_target {
            //     ExportTarget::Paged => ProjectTask::ExportPdf(ExportPdfTask {
            //         export,
            //         pdf_standards: vec![],
            //         creation_timestamp: compile_config.determine_creation_timestamp(),
            //     }),
            //     ExportTarget::Html => ProjectTask::ExportHtml(ExportHtmlTask { export }),
            // },
            task: ProjectTask::ExportPdf(ExportPdfTask {
                export,
                pdf_standards: vec![],
                creation_timestamp: self.creation_timestamp(),
            }),
            count_words: self.notify_status,
        }
    }

    /// Determines the font options.
    pub fn font_opts(&self) -> CompileFontArgs {
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
    pub fn package_opts(&self) -> CompilePackageArgs {
        if let Some(extras) = &self.typst_extra_args {
            return extras.package.clone();
        }
        CompilePackageArgs::default()
    }

    /// Determines the font resolver.
    pub fn fonts(&self) -> Arc<TinymistFontResolver> {
        // todo: on font resolving failure, downgrade to a fake font book
        let font = || {
            let opts = self.font_opts();

            log::info!("creating SharedFontResolver with {opts:?}");
            Derived(
                crate::project::LspUniverseBuilder::resolve_fonts(opts)
                    .map(Arc::new)
                    .expect("failed to create font book"),
            )
        };
        self.fonts.get_or_init(font).clone().0
    }

    /// Determines the `sys.inputs` for the entry file.
    pub fn inputs(&self) -> ImmutDict {
        #[comemo::memoize]
        fn combine(lhs: ImmutDict, rhs: ImmutDict) -> ImmutDict {
            let mut dict = (**lhs).clone();
            for (k, v) in rhs.iter() {
                dict.insert(k.clone(), v.clone());
            }

            Arc::new(LazyHash::new(dict))
        }

        combine(self.user_inputs(), self.lsp_inputs.clone())
    }

    fn user_inputs(&self) -> ImmutDict {
        static EMPTY: Lazy<ImmutDict> = Lazy::new(ImmutDict::default);

        if let Some(extras) = &self.typst_extra_args {
            return extras.inputs.clone();
        }

        EMPTY.clone()
    }

    /// Determines the creation timestamp.
    pub fn creation_timestamp(&self) -> Option<i64> {
        self.typst_extra_args.as_ref()?.creation_timestamp
    }

    /// Determines the certification path.
    pub fn certification_path(&self) -> Option<ImmutPath> {
        self.typst_extra_args.as_ref()?.cert.clone()
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
    /// The locale of the editor.
    pub locale: Option<String>,
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

        let locale = params
            .initialization_options
            .as_ref()
            .and_then(|init| init.get("locale").and_then(|v| v.as_str()))
            .or(params.locale.as_deref());

        Self {
            position_encoding,
            cfg_change_registration: try_or(|| workspace?.configuration, false),
            notify_will_rename_files: try_or(|| file_operations?.will_rename, false),
            tokens_dynamic_registration: try_or(|| sema?.dynamic_registration, false),
            tokens_overlapping_token_support: try_or(|| sema?.overlapping_token_support, false),
            tokens_multiline_token_support: try_or(|| sema?.multiline_token_support, false),
            doc_line_folding_only: try_or(|| fold?.line_folding_only, true),
            doc_fmt_dynamic_registration: try_or(|| format?.dynamic_registration, false),
            locale: locale.map(ToOwned::to_owned),
        }
    }
}

impl From<&dapts::InitializeRequestArguments> for ConstConfig {
    fn from(params: &dapts::InitializeRequestArguments) -> Self {
        let locale = params.locale.as_deref();

        Self {
            locale: locale.map(ToOwned::to_owned),
            ..Default::default()
        }
    }
}

/// Determines in what format paths are specified. The default is `path`, which
/// is the native format.
pub type DapPathFormat = dapts::InitializeRequestArgumentsPathFormat;

/// Configuration set at initialization that won't change within a single DAP
/// session.
#[derive(Debug, Clone)]
pub struct ConstDapConfig {
    /// The format of paths.
    pub path_format: DapPathFormat,
    /// Whether lines start at 1.
    pub lines_start_at1: bool,
    /// Whether columns start at 1.
    pub columns_start_at1: bool,
}

impl Default for ConstDapConfig {
    fn default() -> Self {
        Self::from(&dapts::InitializeRequestArguments::default())
    }
}

impl From<&dapts::InitializeRequestArguments> for ConstDapConfig {
    fn from(params: &dapts::InitializeRequestArguments) -> Self {
        Self {
            path_format: params.path_format.clone().unwrap_or(DapPathFormat::Path),
            lines_start_at1: params.lines_start_at1.unwrap_or(true),
            columns_start_at1: params.columns_start_at1.unwrap_or(true),
        }
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

/// The preview features.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFeat {
    /// The browsing preview options.
    #[serde(default)]
    pub browsing: BrowsingPreviewOpts,
    /// The background preview options.
    #[serde(default)]
    pub background: BackgroundPreviewOpts,
}

/// Options for browsing preview.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsingPreviewOpts {
    /// The arguments for the `tinymist.startDefaultPreview` command.
    pub args: Option<Vec<String>>,
}

/// Options for background preview.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundPreviewOpts {
    /// Whether to run the preview in the background.
    pub enabled: bool,
    /// The arguments for the background preview.
    pub args: Option<Vec<String>>,
}

/// The extra typst arguments passed to the language server. You can pass any
/// arguments as you like, and we will try to follow behaviors of the **same
/// version** of typst-cli.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypstExtraArgs {
    /// The root directory for the compilation routine.
    pub root_dir: Option<ImmutPath>,
    /// The path to the entry.
    pub entry: Option<ImmutPath>,
    /// The additional input arguments to compile the entry file.
    pub inputs: ImmutDict,
    /// The additional font paths.
    pub font: CompileFontArgs,
    /// The package related arguments.
    pub package: CompilePackageArgs,
    /// The creation timestamp for various outputs (in seconds).
    pub creation_timestamp: Option<i64>,
    /// The path to the certification file.
    pub cert: Option<ImmutPath>,
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
        ..SemanticTokensOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tinymist_project::PathPattern;

    fn update_config(config: &mut Config, update: &JsonValue) -> Result<()> {
        temp_env::with_vars_unset(Vec::<String>::new(), || config.update(update))
    }

    fn good_config(config: &mut Config, update: &JsonValue) {
        update_config(config, update).expect("not good");
        assert!(config.warnings.is_empty(), "{:?}", config.warnings);
    }

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

        good_config(&mut config, &update);

        // Nix specifies this environment variable when testing.
        let has_source_date_epoch = std::env::var("SOURCE_DATE_EPOCH").is_ok();
        if has_source_date_epoch {
            let args = config.typst_extra_args.as_mut().unwrap();
            assert!(args.creation_timestamp.is_some());
            args.creation_timestamp = None;
        }

        assert_eq!(config.output_path, PathPattern::new("out"));
        assert_eq!(config.export_pdf, TaskWhen::OnSave);
        assert_eq!(
            config.entry_resolver.root_path,
            Some(ImmutPath::from(root_path))
        );
        assert_eq!(config.semantic_tokens, SemanticTokensMode::Enable);
        assert_eq!(config.formatter_mode, FormatterMode::Typstyle);
        assert_eq!(
            config.typst_extra_args,
            Some(TypstExtraArgs {
                root_dir: Some(ImmutPath::from(root_path)),
                ..TypstExtraArgs::default()
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

        good_config(&mut config, &update);

        assert_eq!(config.export_pdf, TaskWhen::OnType);
    }

    #[test]
    fn test_config_creation_timestamp() {
        type Timestamp = Option<i64>;

        fn timestamp(f: impl FnOnce(&mut Config)) -> Timestamp {
            let mut config = Config::default();

            f(&mut config);

            let args = config.typst_extra_args;
            args.and_then(|args| args.creation_timestamp)
        }

        // assert!(timestamp(|_| {}).is_none());
        // assert!(timestamp(|config| {
        //     let update = json!({});
        //     good_config(&mut config, &update);
        // })
        // .is_none());

        let args_timestamp = timestamp(|config| {
            let update = json!({
                "typstExtraArgs": ["--creation-timestamp", "1234"]
            });
            good_config(config, &update);
        });
        assert!(args_timestamp.is_some());

        // todo: concurrent get/set env vars is unsafe
        //     std::env::set_var("SOURCE_DATE_EPOCH", "1234");
        //     let env_timestamp = timestamp(|config| {
        //         update_config(&mut config, &json!({})).unwrap();
        //     });

        //     assert_eq!(args_timestamp, env_timestamp);
    }

    #[test]
    fn test_empty_extra_args() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": []
        });

        good_config(&mut config, &update);
    }

    #[test]
    fn test_null_completion() {
        let mut config = Config::default();
        let update = json!({
            "completion": null
        });

        good_config(&mut config, &update);
    }

    #[test]
    fn test_null_root() {
        let mut config = Config::default();
        let update = json!({
            "root": null
        });

        good_config(&mut config, &update);
    }

    #[test]
    fn test_null_extra_args() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": null
        });

        good_config(&mut config, &update);
    }

    #[test]
    fn test_font_opts() {
        fn opts(update: Option<&JsonValue>) -> CompileFontArgs {
            let mut config = Config::default();
            if let Some(update) = update {
                good_config(&mut config, update);
            }

            config.font_opts()
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

        let err = format!("{}", update_config(&mut config, &update).unwrap_err());
        assert!(err.contains("absolute path"), "unexpected error: {err}");
    }

    #[test]
    fn test_reject_abnormal_root2() {
        let mut config = Config::default();
        let update = json!({
            "typstExtraArgs": ["--root", "."]
        });

        let err = format!("{}", update_config(&mut config, &update).unwrap_err());
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
            update_config(&mut config, &update).expect("updated");
            // Passing it twice doesn't affect the result.
            update_config(&mut config, &update).expect("updated");
            config
        };
        {
            let mut config = Config::default();
            let update = json!({
                "typstExtraArgs": ["main.typ", "main.typ"]
            });
            update_config(&mut config, &update).unwrap();
            let warns = format!("{:?}", config.warnings);
            assert!(warns.contains("typstExtraArgs"), "warns: {warns}");
            assert!(warns.contains(r#"String(\"main.typ\")"#), "warns: {warns}");
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
            update_config(&mut config, &update).expect("updated");
            // Passing it twice doesn't affect the result.
            update_config(&mut config, &update).expect("updated");

            assert_eq!(config.typst_extra_args, simple_config.typst_extra_args);
        }
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

    #[test]
    fn test_typstyle_formatting_config_set_tab_spaces() {
        let config = Config {
            formatter_mode: FormatterMode::Typstyle,
            formatter_indent_size: Some(8),
            ..Config::default()
        };
        let config = config.formatter();
        assert_eq!(config.position_encoding, PositionEncoding::Utf16);

        let typstyle_config = match config.config {
            FormatterConfig::Typstyle(e) => e,
            _ => panic!("unexpected configuration of formatter"),
        };

        assert_eq!(typstyle_config.tab_spaces, 8);
    }

    #[test]
    fn test_default_lsp_config_initialize() {
        let (_conf, err) =
            Config::extract_lsp_params(InitializeParams::default(), CompileFontArgs::default());
        assert!(err.is_none());
    }

    #[test]
    fn test_default_dap_config_initialize() {
        let (_conf, err) = Config::extract_dap_params(
            dapts::InitializeRequestArguments::default(),
            CompileFontArgs::default(),
        );
        assert!(err.is_none());
    }

    #[test]
    fn test_config_package_path_from_env() {
        let pkg_path = Path::new(if cfg!(windows) { "C:\\pkgs" } else { "/pkgs" });

        temp_env::with_var("TYPST_PACKAGE_CACHE_PATH", Some(pkg_path), || {
            let (conf, err) =
                Config::extract_lsp_params(InitializeParams::default(), CompileFontArgs::default());
            assert!(err.is_none());
            let applied_cache_path = conf
                .typst_extra_args
                .is_some_and(|args| args.package.package_cache_path == Some(pkg_path.into()));
            assert!(applied_cache_path);
        });
    }
}
