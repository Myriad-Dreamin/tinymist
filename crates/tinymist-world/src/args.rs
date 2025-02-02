use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use clap::{builder::ValueParser, ArgAction, Parser};
use serde::{Deserialize, Serialize};
use tinymist_std::{bail, error::prelude::*};
use tinymist_vfs::ImmutDict;
use typst::{foundations::IntoValue, utils::LazyHash};

use crate::EntryOpts;

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

/// Arguments related to where packages are stored in the system.
#[derive(Debug, Clone, Parser, Default, PartialEq, Eq)]
pub struct CompilePackageArgs {
    /// Custom path to local packages, defaults to system-dependent location
    #[clap(long = "package-path", env = "TYPST_PACKAGE_PATH", value_name = "DIR")]
    pub package_path: Option<PathBuf>,

    /// Custom path to package cache, defaults to system-dependent location
    #[clap(
        long = "package-cache-path",
        env = "TYPST_PACKAGE_CACHE_PATH",
        value_name = "DIR"
    )]
    pub package_cache_path: Option<PathBuf>,
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

    /// Package related arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,

    /// The document's creation date formatted as a UNIX timestamp (in seconds).
    ///
    /// For more information, see <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[clap(
        long = "creation-timestamp",
        env = "SOURCE_DATE_EPOCH",
        value_name = "UNIX_TIMESTAMP",
        value_parser = parse_source_date_epoch,
        hide(true),
    )]
    pub creation_timestamp: Option<i64>,

    /// Path to CA certificate file for network access, especially for
    /// downloading typst packages.
    #[clap(long = "cert", env = "TYPST_CERT", value_name = "CERT_PATH")]
    pub cert: Option<PathBuf>,
}

impl CompileOnceArgs {
    pub fn resolve_inputs(&self) -> Option<ImmutDict> {
        if self.inputs.is_empty() {
            return None;
        }

        let pairs = self.inputs.iter();
        let pairs = pairs.map(|(k, v)| (k.as_str().into(), v.as_str().into_value()));
        Some(Arc::new(LazyHash::new(pairs.collect())))
    }

    /// Resolves the entry options.
    pub fn resolve_sys_entry_opts(&self) -> Result<EntryOpts> {
        let mut cwd = None;
        let mut cwd = move || {
            cwd.get_or_insert_with(|| {
                std::env::current_dir().context("failed to get current directory")
            })
            .clone()
        };

        let main = {
            let input = self.input.as_ref().context("entry file must be provided")?;
            let input = Path::new(&input);
            if input.is_absolute() {
                input.to_owned()
            } else {
                cwd()?.join(input)
            }
        };

        let root = if let Some(root) = &self.root {
            if root.is_absolute() {
                root.clone()
            } else {
                cwd()?.join(root)
            }
        } else {
            main.parent()
                .context("entry file don't have a valid parent as root")?
                .to_owned()
        };

        let relative_main = match main.strip_prefix(&root) {
            Ok(relative_main) => relative_main,
            Err(_) => {
                log::error!("entry file must be inside the root, file: {main:?}, root: {root:?}");
                bail!("entry file must be inside the root, file: {main:?}, root: {root:?}");
            }
        };

        Ok(EntryOpts::new_rooted(
            root.clone(),
            Some(relative_main.to_owned()),
        ))
    }
}

#[cfg(feature = "system")]
impl CompileOnceArgs {
    /// Resolves the arguments into a system universe. This is also a sample
    /// implementation of how to resolve the arguments (user inputs) into a
    /// universe.
    pub fn resolve_system(&self) -> Result<crate::TypstSystemUniverse> {
        use crate::system::SystemUniverseBuilder;

        let entry = self.resolve_sys_entry_opts()?.try_into()?;
        let inputs = self.resolve_inputs().unwrap_or_default();
        let fonts = Arc::new(SystemUniverseBuilder::resolve_fonts(self.font.clone())?);
        let package = SystemUniverseBuilder::resolve_package(
            self.cert.as_deref().map(From::from),
            Some(&self.package),
        );

        Ok(SystemUniverseBuilder::build(entry, inputs, fonts, package))
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
pub fn parse_source_date_epoch(raw: &str) -> Result<i64, String> {
    raw.parse()
        .map_err(|err| format!("timestamp must be decimal integer ({err})"))
}

/// Parses a UNIX timestamp according to <https://reproducible-builds.org/specs/source-date-epoch/>
pub fn convert_source_date_epoch(seconds: i64) -> Result<chrono::DateTime<Utc>, String> {
    DateTime::from_timestamp(seconds, 0).ok_or_else(|| "timestamp out of range".to_string())
}
