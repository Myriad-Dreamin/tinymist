//! World implementation of typst for tinymist.

pub use reflexo_typst::config::CompileFontOpts;
pub use reflexo_typst::error::prelude;
pub use reflexo_typst::font::FontResolverImpl;
pub use reflexo_typst::world as base;
pub use reflexo_typst::{entry::*, font, vfs, EntryOpts, EntryState};

use std::path::Path;
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use ::typst::utils::LazyHash;
use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{builder::ValueParser, ArgAction, Parser};
use reflexo_typst::error::prelude::*;
use reflexo_typst::font::system::SystemFontSearcher;
use reflexo_typst::foundations::{Str, Value};
use reflexo_typst::vfs::{system::SystemAccessModel, Vfs};
use reflexo_typst::{CompilerFeat, CompilerUniverse, CompilerWorld, TypstDict};
use serde::{Deserialize, Serialize};

pub mod https;
use https::HttpsRegistry;

const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// Compiler feature for LSP universe and worlds without typst.ts to implement
/// more for tinymist. type trait of [`TypstSystemWorld`].
#[derive(Debug, Clone, Copy)]
pub struct SystemCompilerFeatExtend;

impl CompilerFeat for SystemCompilerFeatExtend {
    /// Uses [`FontResolverImpl`] directly.
    type FontResolver = FontResolverImpl;
    /// It accesses a physical file system.
    type AccessModel = SystemAccessModel;
    /// It performs native HTTP requests for fetching package data.
    type Registry = HttpsRegistry;
}

/// The compiler universe in system environment.
pub type TypstSystemUniverseExtend = CompilerUniverse<SystemCompilerFeatExtend>;
/// The compiler world in system environment.
pub type TypstSystemWorldExtend = CompilerWorld<SystemCompilerFeatExtend>;

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

    /// Path to CA certificate file for network access, especially for
    /// downloading typst packages.
    #[clap(long = "cert", env = "TYPST_CERT", value_name = "CERT_PATH")]
    pub certification: Option<PathBuf>,
}

impl CompileOnceArgs {
    /// Get a universe instance from the given arguments.
    pub fn resolve(&self) -> anyhow::Result<LspUniverse> {
        let entry = self.entry()?.try_into()?;
        let fonts = LspUniverseBuilder::resolve_fonts(self.font.clone())?;
        let inputs = self
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
            .collect();
        let cert_path = self.certification.clone();

        LspUniverseBuilder::build(
            entry,
            Arc::new(fonts),
            Arc::new(LazyHash::new(inputs)),
            cert_path,
        )
        .context("failed to create universe")
    }

    /// Get the entry options from the arguments.
    pub fn entry(&self) -> anyhow::Result<EntryOpts> {
        let input = self.input.as_ref().context("entry file must be provided")?;
        let input = Path::new(&input);
        let entry = if input.is_absolute() {
            input.to_owned()
        } else {
            std::env::current_dir().unwrap().join(input)
        };

        let root = if let Some(root) = &self.root {
            if root.is_absolute() {
                root.clone()
            } else {
                std::env::current_dir().unwrap().join(root)
            }
        } else {
            std::env::current_dir().unwrap()
        };

        if !entry.starts_with(&root) {
            log::error!("entry file must be in the root directory");
            std::process::exit(1);
        }

        let relative_entry = match entry.strip_prefix(&root) {
            Ok(e) => e,
            Err(_) => {
                log::error!("entry path must be inside the root: {}", entry.display());
                std::process::exit(1);
            }
        };

        Ok(EntryOpts::new_rooted(
            root.clone(),
            Some(relative_entry.to_owned()),
        ))
    }
}

/// Compiler feature for LSP universe and worlds.
pub type LspCompilerFeat = SystemCompilerFeatExtend;
/// LSP universe that spawns LSP worlds.
pub type LspUniverse = TypstSystemUniverseExtend;
/// LSP world.
pub type LspWorld = TypstSystemWorldExtend;
/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<LazyHash<TypstDict>>;

/// Builder for LSP universe.
pub struct LspUniverseBuilder;

impl LspUniverseBuilder {
    /// Create [`LspUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    pub fn build(
        entry: EntryState,
        font_resolver: Arc<FontResolverImpl>,
        inputs: ImmutDict,
        cert_path: Option<PathBuf>,
    ) -> ZResult<LspUniverse> {
        Ok(LspUniverse::new_raw(
            entry,
            Some(inputs),
            Vfs::new(SystemAccessModel {}),
            HttpsRegistry::new(cert_path),
            font_resolver,
        ))
    }

    /// Resolve fonts from given options.
    pub fn resolve_fonts(args: CompileFontArgs) -> ZResult<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
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
pub fn parse_source_date_epoch(raw: &str) -> Result<DateTime<Utc>, String> {
    let timestamp: i64 = raw
        .parse()
        .map_err(|err| format!("timestamp must be decimal integer ({err})"))?;
    DateTime::from_timestamp(timestamp, 0).ok_or_else(|| "timestamp out of range".to_string())
}
