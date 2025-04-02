use core::fmt;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{builder::ValueParser, ArgAction, Parser, ValueEnum};
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

    /// Font related arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,

    /// Package related arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,

    /// Enables in-development features that may be changed or removed at any
    /// time.
    #[arg(long = "features", value_delimiter = ',', env = "TYPST_FEATURES")]
    pub features: Vec<Feature>,

    /// Add a string key-value pair visible through `sys.inputs`
    #[clap(
        long = "input",
        value_name = "key=value",
        action = ArgAction::Append,
        value_parser = ValueParser::new(parse_input_pair),
    )]
    pub inputs: Vec<(String, String)>,

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

    /// One (or multiple comma-separated) PDF standards that Typst will enforce
    /// conformance with.
    #[arg(long = "pdf-standard", value_delimiter = ',')]
    pub pdf_standard: Vec<PdfStandard>,

    /// Path to CA certificate file for network access, especially for
    /// downloading typst packages.
    #[clap(long = "cert", env = "TYPST_CERT", value_name = "CERT_PATH")]
    pub cert: Option<PathBuf>,
}

impl CompileOnceArgs {
    pub fn resolve_features(&self) -> typst::Features {
        typst::Features::from_iter(self.features.iter().map(|f| (*f).into()))
    }

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

macro_rules! display_possible_values {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.to_possible_value()
                    .expect("no values are skipped")
                    .get_name()
                    .fmt(f)
            }
        }
    };
}

/// When to export an output file.
///
/// By default, a `tinymist compile` only provides input information and
/// doesn't change the `when` field. However, you can still specify a `when`
/// argument to override the default behavior for specific tasks.
///
/// ## Examples
///
/// ```bash
/// tinymist compile --when onSave main.typ
/// alias typst="tinymist compile --when=onSave"
/// typst compile main.typ
/// ```
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Hash, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[clap(rename_all = "camelCase")]
pub enum TaskWhen {
    /// Never watch to run task.
    #[default]
    Never,
    /// Run task on saving the document, i.e. on `textDocument/didSave` events.
    OnSave,
    /// Run task on typing, i.e. on `textDocument/didChange` events.
    OnType,
    /// *DEPRECATED* Run task when a document has a title and on saved, which is
    /// useful to filter out template files.
    ///
    /// Note: this is deprecating.
    OnDocumentHasTitle,
}

impl TaskWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, TaskWhen::Never)
    }
}

display_possible_values!(TaskWhen);

/// Which format to use for the generated output file.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
pub enum OutputFormat {
    /// Export to PDF.
    Pdf,
    /// Export to PNG.
    Png,
    /// Export to SVG.
    Svg,
    /// Export to HTML.
    Html,
}

display_possible_values!(OutputFormat);

/// Specifies the current export target.
///
/// The design of this configuration is not yet finalized and for this reason it
/// is guarded behind the html feature. Visit the HTML documentation page for
/// more details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportTarget {
    /// The current export target is for PDF, PNG, and SVG export.
    #[default]
    Paged,
    /// The current export target is for HTML export.
    Html,
}

/// A PDF standard that Typst can enforce conformance with.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, ValueEnum, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum PdfStandard {
    /// PDF 1.7.
    #[value(name = "1.7")]
    #[serde(rename = "1.7")]
    V_1_7,
    /// PDF/A-2b.
    #[value(name = "a-2b")]
    #[serde(rename = "a-2b")]
    A_2b,
    /// PDF/A-3b.
    #[value(name = "a-3b")]
    #[serde(rename = "a-3b")]
    A_3b,
}

display_possible_values!(PdfStandard);

/// An in-development feature that may be changed or removed at any time.
#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum Feature {
    Html,
}

display_possible_values!(Feature);

impl From<Feature> for typst::Feature {
    fn from(f: Feature) -> typst::Feature {
        match f {
            Feature::Html => typst::Feature::Html,
        }
    }
}
