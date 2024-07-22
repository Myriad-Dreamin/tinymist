use std::{borrow::Cow, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use clap::{builder::ValueParser, ArgAction, Parser};
use comemo::Prehashed;
use serde::{Deserialize, Serialize};
use typst_ts_core::{
    config::{compiler::EntryState, CompileFontOpts as FontOptsInner},
    error::prelude::*,
    font::FontResolverImpl,
    TypstDict,
};

use typst_ts_compiler::{
    font::system::SystemFontSearcher,
    package::http::HttpRegistry,
    vfs::{system::SystemAccessModel, Vfs},
    SystemCompilerFeat, TypstSystemUniverse, TypstSystemWorld,
};

const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// The font arguments for the compiler.
#[derive(Debug, Clone, Default, Parser, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileFontArgs {
    /// Font paths
    #[clap(
        long = "font-path",
        value_name = "DIR",
        action = clap::ArgAction::Append,
        env = "TYPST_FONT_PATHS",
        value_delimiter = ENV_PATH_SEP
    )]
    pub font_paths: Vec<PathBuf>,

    /// Ensures system fonts won't be searched, unless explicitly included via
    /// `--font-path`
    #[clap(long, default_value = "false")]
    pub ignore_system_fonts: bool,
}

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileOnceArgs {
    /// Path to input Typst file
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

    /// Font related arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,

    /// The document's creation date formatted as a UNIX timestamp.
    ///
    /// For more information, see <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[clap(
        long = "creation-timestamp",
        env = "SOURCE_DATE_EPOCH",
        value_name = "UNIX_TIMESTAMP",
        value_parser = parse_source_date_epoch,
        hide(true),
    )]
    pub creation_timestamp: Option<DateTime<Utc>>,
}

/// Compiler feature for LSP world.
pub type LspCompilerFeat = SystemCompilerFeat;
/// LSP universe that spawns LSP worlds.
pub type LspUniverse = TypstSystemUniverse;
/// LSP world.
pub type LspWorld = TypstSystemWorld;
/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<Prehashed<TypstDict>>;

/// Builder for LSP world.
pub struct LspWorldBuilder;

impl LspWorldBuilder {
    /// Create [`LspUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    pub fn build(
        entry: EntryState,
        font_resolver: Arc<FontResolverImpl>,
        inputs: ImmutDict,
    ) -> ZResult<LspUniverse> {
        Ok(LspUniverse::new_raw(
            entry,
            Some(inputs),
            Vfs::new(SystemAccessModel {}),
            HttpRegistry::default(),
            font_resolver,
        ))
    }

    /// Resolve fonts from given options.
    pub(crate) fn resolve_fonts(args: CompileFontArgs) -> ZResult<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(FontOptsInner {
            font_profile_cache_path: Default::default(),
            font_paths: args.font_paths,
            no_system_fonts: args.ignore_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }
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

/// Parses a UNIX timestamp according to <https://reproducible-builds.org/specs/source-date-epoch/>
fn parse_source_date_epoch(raw: &str) -> Result<DateTime<Utc>, String> {
    let timestamp: i64 = raw
        .parse()
        .map_err(|err| format!("timestamp must be decimal integer ({err})"))?;
    DateTime::from_timestamp(timestamp, 0).ok_or_else(|| "timestamp out of range".to_string())
}
