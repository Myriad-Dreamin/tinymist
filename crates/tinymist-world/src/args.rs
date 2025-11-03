//! Shared arguments to create a world.

use core::fmt;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{ArgAction, Parser, ValueEnum, builder::ValueParser};
use serde::{Deserialize, Serialize};
use tinymist_std::{bail, error::prelude::*};
use tinymist_vfs::ImmutDict;
use typst::{foundations::IntoValue, utils::LazyHash};

use crate::EntryOpts;

const ENV_PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// The font arguments for the world to specify the way to search for fonts.
#[derive(Debug, Clone, Default, Parser, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileFontArgs {
    /// Add additional directories that are recursively searched for fonts.
    ///
    /// If multiple paths are specified, they are separated by the system's path
    /// separator (`:` on Unix-like systems and `;` on Windows).
    #[clap(
        long = "font-path",
        value_name = "DIR",
        action = clap::ArgAction::Append,
        env = "TYPST_FONT_PATHS",
        value_delimiter = ENV_PATH_SEP
    )]
    pub font_paths: Vec<PathBuf>,

    /// Ensure system fonts won't be searched, unless explicitly included via
    /// `--font-path`.
    #[clap(long, default_value = "false")]
    pub ignore_system_fonts: bool,
}

/// The package arguments for the world to specify where packages are stored in
/// the system.
#[derive(Debug, Clone, Parser, Default, PartialEq, Eq)]
pub struct CompilePackageArgs {
    /// Specify a custom path to local packages, defaults to system-dependent
    /// location.
    #[clap(long = "package-path", env = "TYPST_PACKAGE_PATH", value_name = "DIR")]
    pub package_path: Option<PathBuf>,

    /// Specify a custom path to package cache, defaults to system-dependent
    /// location.
    #[clap(
        long = "package-cache-path",
        env = "TYPST_PACKAGE_CACHE_PATH",
        value_name = "DIR"
    )]
    pub package_cache_path: Option<PathBuf>,
}

/// Common arguments to create a world (environment) to run typst tasks, e.g.
/// `compile`, `watch`, and `query`.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileOnceArgs {
    /// Specify the path to input Typst file. If the path is relative, it will
    /// be resolved relative to the current working directory (PWD).
    #[clap(value_name = "INPUT")]
    pub input: Option<String>,

    /// Configure the project root (for absolute paths).
    #[clap(long = "root", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Specify the font related arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,

    /// Specify the package related arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,

    /// Specify the PDF export related arguments.
    #[clap(flatten)]
    pub pdf: PdfExportArgs,

    /// Specify the PNG export related arguments.
    #[clap(flatten)]
    pub png: PngExportArgs,

    /// Enable in-development features that may be changed or removed at any
    /// time.
    #[arg(long = "features", value_delimiter = ',', env = "TYPST_FEATURES")]
    pub features: Vec<Feature>,

    /// Add a string key-value pair visible through `sys.inputs`.
    ///
    /// ### Examples
    ///
    /// Tell the script that `sys.inputs.foo` is `"bar"` (type: `str`).
    ///
    /// ```bash
    /// tinymist compile --input foo=bar
    /// ```
    #[clap(
        long = "input",
        value_name = "key=value",
        action = ArgAction::Append,
        value_parser = ValueParser::new(parse_input_pair),
    )]
    pub inputs: Vec<(String, String)>,

    /// Configure the document's creation date formatted as a UNIX timestamp
    /// (in seconds).
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

    /// Specify the path to CA certificate file for network access, especially
    /// for downloading typst packages.
    #[clap(long = "cert", env = "TYPST_CERT", value_name = "CERT_PATH")]
    pub cert: Option<PathBuf>,
}

impl CompileOnceArgs {
    /// Resolves the features.
    pub fn resolve_features(&self) -> typst::Features {
        typst::Features::from_iter(self.features.iter().map(|f| (*f).into()))
    }

    /// Resolves the inputs.
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

/// Specify the PDF export related arguments.
#[derive(Debug, Clone, Parser, Default)]
pub struct PdfExportArgs {
    /// Specify the PDF standards that Typst will enforce conformance with.
    ///
    /// If multiple standards are specified, they are separated by commas.
    #[arg(long = "pdf-standard", value_delimiter = ',')]
    pub standard: Vec<PdfStandard>,

    /// By default, even when not producing a `PDF/UA-1` document, a tagged PDF
    /// document is written to provide a baseline of accessibility. In some
    /// circumstances (for example when trying to reduce the size of a document)
    /// it can be desirable to disable tagged PDF.
    #[arg(long = "no-pdf-tags")]
    pub no_tags: bool,
}

/// Specify the PNG export related arguments.
#[derive(Debug, Clone, Parser, Default)]
pub struct PngExportArgs {
    /// Specify the PPI (pixels per inch) to use for PNG export.
    #[arg(long = "ppi", default_value_t = 144.0)]
    pub ppi: f32,
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

/// Configure when to run a task.
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
#[derive(Debug, Clone, Eq, PartialEq, Default, Hash, ValueEnum, Serialize, Deserialize)]
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
    /// Checks by running a typst script.
    Script,
}

impl TaskWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, TaskWhen::Never)
    }
}

display_possible_values!(TaskWhen);

/// Configure the format of the output file.
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

/// Configure the current export target.
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
    /// PDF 1.4.
    #[value(name = "1.4")]
    #[serde(rename = "1.4")]
    V_1_4,
    /// PDF 1.5.
    #[value(name = "1.5")]
    #[serde(rename = "1.5")]
    V_1_5,
    /// PDF 1.6.
    #[value(name = "1.6")]
    #[serde(rename = "1.6")]
    V_1_6,
    /// PDF 1.7.
    #[value(name = "1.7")]
    #[serde(rename = "1.7")]
    V_1_7,
    /// PDF 2.0.
    #[value(name = "2.0")]
    #[serde(rename = "2.0")]
    V_2_0,
    /// PDF/A-1b.
    #[value(name = "a-1b")]
    #[serde(rename = "a-1b")]
    A_1b,
    /// PDF/A-1a.
    #[value(name = "a-1a")]
    #[serde(rename = "a-1a")]
    A_1a,
    /// PDF/A-2b.
    #[value(name = "a-2b")]
    #[serde(rename = "a-2b")]
    A_2b,
    /// PDF/A-2u.
    #[value(name = "a-2u")]
    #[serde(rename = "a-2u")]
    A_2u,
    /// PDF/A-2a.
    #[value(name = "a-2a")]
    #[serde(rename = "a-2a")]
    A_2a,
    /// PDF/A-3b.
    #[value(name = "a-3b")]
    #[serde(rename = "a-3b")]
    A_3b,
    /// PDF/A-3u.
    #[value(name = "a-3u")]
    #[serde(rename = "a-3u")]
    A_3u,
    /// PDF/A-3a.
    #[value(name = "a-3a")]
    #[serde(rename = "a-3a")]
    A_3a,
    /// PDF/A-4.
    #[value(name = "a-4")]
    #[serde(rename = "a-4")]
    A_4,
    /// PDF/A-4f.
    #[value(name = "a-4f")]
    #[serde(rename = "a-4f")]
    A_4f,
    /// PDF/A-4e.
    #[value(name = "a-4e")]
    #[serde(rename = "a-4e")]
    A_4e,
    /// PDF/UA-1.
    #[value(name = "ua-1")]
    #[serde(rename = "ua-1")]
    Ua_1,
}

display_possible_values!(PdfStandard);

/// An in-development feature that may be changed or removed at any time.
#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum Feature {
    /// The HTML feature.
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
