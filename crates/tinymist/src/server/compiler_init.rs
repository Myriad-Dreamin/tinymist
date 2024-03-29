use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use clap::builder::ValueParser;
use clap::{ArgAction, Parser};
use comemo::Prehashed;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{DiagnosticsMap, PositionEncoding};
use tokio::sync::mpsc;
use typst::foundations::IntoValue;
use typst::syntax::FileId;
use typst::syntax::VirtualPath;
use typst::util::Deferred;
use typst_ts_core::config::compiler::EntryState;
use typst_ts_core::{ImmutPath, TypstDict};

use crate::compiler::{CompileServer, CompileServerArgs};
use crate::harness::LspDriver;
use crate::world::{ImmutDict, SharedFontResolver};
use crate::{CompileExtraOpts, CompileFontOpts, ExportMode, LspHost};

#[cfg(feature = "clap")]
const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct FontArgs {
    /// Font paths, which doesn't allow for dynamic configuration
    #[cfg_attr(feature = "clap", clap(
        long = "font-path",
        value_name = "DIR",
        action = clap::ArgAction::Append,
        env = "TYPST_FONT_PATHS",
        value_delimiter = ENV_PATH_SEP
    ))]
    pub font_paths: Vec<PathBuf>,
    /// Exclude system fonts
    #[cfg_attr(feature = "clap", clap(long, default_value = "false"))]
    pub no_system_fonts: bool,
}

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileOnceArgs {
    /// Path to input Typst file, use `-` to read input from stdin
    #[clap(value_name = "INPUT")]
    pub input: Option<String>,

    /// Configures the project root (for absolute paths)
    #[clap(long = "root", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Add a string key-value pair visible through `sys.inputs`
    #[clap(
        long = "input",
        value_name = "key=value",
        action = ArgAction::Append,
        value_parser = ValueParser::new(parse_input_pair),
    )]
    pub inputs: Vec<(String, String)>,

    #[cfg_attr(feature = "clap", clap(flatten))]
    pub font: FontArgs,
}

/// Parses key/value pairs split by the first equal sign.
///
/// This function will return an error if the argument contains no equals sign
/// or contains the key (before the equals sign) is empty.
fn parse_input_pair(raw: &str) -> Result<(String, String), String> {
    let (key, val) = raw
        .split_once('=')
        .ok_or("input must be a key and a value separated by an equal sign")?;
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err("the key was missing or empty".to_owned());
    }
    let val = val.trim().to_owned();
    Ok((key, val))
}

/// The user configuration read from the editor.
#[derive(Debug, Default, Clone)]
pub struct CompileConfig {
    /// The workspace roots from initialization.
    pub roots: Vec<PathBuf>,
    /// The output directory for PDF export.
    pub output_path: String,
    /// The mode of PDF export.
    pub export_pdf: ExportMode,
    /// Specifies the root path of the project manually.
    pub root_path: Option<PathBuf>,
    /// Typst extra arguments.
    pub typst_extra_args: Option<CompileExtraOpts>,
    pub has_default_entry_path: bool,
}

impl CompileConfig {
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
        if let Some(JsonValue::String(output_path)) = update.get("outputPath") {
            self.output_path = output_path.to_owned();
        } else {
            self.output_path = String::new();
        }

        let export_pdf = update
            .get("exportPdf")
            .map(ExportMode::deserialize)
            .and_then(Result::ok);
        if let Some(export_pdf) = export_pdf {
            self.export_pdf = export_pdf;
        } else {
            self.export_pdf = ExportMode::default();
        }

        let root_path = update.get("rootPath");
        if let Some(root_path) = root_path {
            if root_path.is_null() {
                self.root_path = None;
            }
            if let Some(root_path) = root_path.as_str().map(PathBuf::from) {
                self.root_path = Some(root_path);
            }
        } else {
            self.root_path = None;
        }

        'parse_extra_args: {
            if let Some(typst_extra_args) = update.get("typstExtraArgs") {
                let typst_args: Vec<String> = match serde_json::from_value(typst_extra_args.clone())
                {
                    Ok(e) => e,
                    Err(e) => {
                        log::error!("failed to parse typstExtraArgs: {e}");
                        return Ok(());
                    }
                };

                let command = match CompileOnceArgs::try_parse_from(
                    Some("typst-cli".to_owned()).into_iter().chain(typst_args),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        log::error!("failed to parse typstExtraArgs: {e}");
                        break 'parse_extra_args;
                    }
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
                    font_paths: command.font.font_paths,
                });
            }
        }

        self.has_default_entry_path = self.determine_default_entry_path().is_some();
        self.validate()?;

        Ok(())
    }

    pub fn determine_root(&self, entry: Option<&ImmutPath>) -> Option<ImmutPath> {
        if let Some(path) = &self.root_path {
            return Some(path.as_path().into());
        }

        if let Some(extras) = &self.typst_extra_args {
            if let Some(root) = &extras.root_dir {
                return Some(root.as_path().into());
            }
        }

        if let Some(path) = &self
            .typst_extra_args
            .as_ref()
            .and_then(|x| x.root_dir.clone())
        {
            return Some(path.as_path().into());
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

    pub fn determine_default_entry_path(&self) -> Option<ImmutPath> {
        self.typst_extra_args.as_ref().and_then(|e| {
            if let Some(e) = &e.entry {
                if e.is_relative() {
                    let root = self.determine_root(None)?;
                    return Some(root.join(e).as_path().into());
                }
            }
            e.entry.clone()
        })
    }

    pub fn determine_entry(&self, entry: Option<ImmutPath>) -> EntryState {
        // todo: don't ignore entry from typst_extra_args
        // entry: command.input,

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
            // todo
            None => EntryState::new_detached(),
        })
    }

    pub fn determine_inputs(&self) -> ImmutDict {
        static EMPTY: Lazy<ImmutDict> = Lazy::new(ImmutDict::default);

        if let Some(extras) = &self.typst_extra_args {
            return extras.inputs.clone();
        }

        EMPTY.clone()
    }

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

/// Configuration set at initialization that won't change within a single
/// session.
#[derive(Debug, Clone)]
pub struct CompilerConstConfig {
    /// Determined position encoding, either UTF-8 or UTF-16.
    /// Defaults to UTF-16 if not specified.
    pub position_encoding: PositionEncoding,
}

impl Default for CompilerConstConfig {
    fn default() -> Self {
        Self {
            position_encoding: PositionEncoding::Utf16,
        }
    }
}

pub struct CompileInit {
    pub handle: tokio::runtime::Handle,
    pub font: CompileFontOpts,
    pub diag_tx: mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>,
}

#[derive(Debug, Deserialize)]
pub struct CompileInitializeParams {
    pub config: serde_json::Value,
    pub position_encoding: Option<lsp_types::PositionEncodingKind>,
}

impl LspDriver for CompileInit {
    type InitParams = CompileInitializeParams;
    type InitResult = ();
    type InitializedSelf = CompileServer;

    fn initialize(
        self,
        client: LspHost<Self::InitializedSelf>,
        params: Self::InitParams,
    ) -> (
        Self::InitializedSelf,
        Result<Self::InitResult, lsp_server::ResponseError>,
    ) {
        let mut compile_config = CompileConfig::default();
        compile_config.update(&params.config).unwrap();

        // prepare fonts
        // todo: on font resolving failure, downgrade to a fake font book
        let font = {
            let mut opts = self.font;
            if let Some(font_paths) = compile_config
                .typst_extra_args
                .as_ref()
                .map(|x| &x.font_paths)
            {
                opts.font_paths = font_paths.clone();
            }

            Deferred::new(|| SharedFontResolver::new(opts).expect("failed to create font book"))
        };

        let args = CompileServerArgs {
            client,
            compile_config,
            const_config: CompilerConstConfig {
                position_encoding: params
                    .position_encoding
                    .map(|x| match x.as_str() {
                        "utf-16" => PositionEncoding::Utf16,
                        _ => PositionEncoding::Utf8,
                    })
                    .unwrap_or_default(),
            },
            diag_tx: self.diag_tx,
            handle: self.handle,
            font,
        };

        let mut service = CompileServer::new(args);

        let primary = service.server(
            "primary".to_owned(),
            service.config.determine_entry(None),
            service.config.determine_inputs(),
        );
        if service.compiler.is_some() {
            panic!("primary already initialized");
        }
        service.compiler = Some(primary);

        (service, Ok(()))
    }
}
