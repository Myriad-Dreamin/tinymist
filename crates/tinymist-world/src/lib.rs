//! World implementation of typst for tinymist.

pub use reflexo_typst::config::CompileFontOpts;
pub use reflexo_typst::error::prelude;
pub use reflexo_typst::font::FontResolverImpl;
pub use reflexo_typst::world as base;
pub use reflexo_typst::{entry::*, font, vfs, EntryOpts, EntryState};

use std::path::Path;
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::{builder::ValueParser, ArgAction, Parser};
use comemo::Prehashed;
use reflexo_typst::error::prelude::*;
use reflexo_typst::font::system::SystemFontSearcher;
use reflexo_typst::foundations::{Str, Value};
use reflexo_typst::vfs::{system::SystemAccessModel, Vfs};
use reflexo_typst::{CompilerFeat, CompilerUniverse, CompilerWorld, TypstDict};
use serde::{Deserialize, Serialize};

use std::sync::OnceLock;
use reflexo_typst::typst::{
    diag::{eco_format, EcoString},
    syntax::package::PackageVersion,
};
use reqwest::{
    blocking::Response,
    Certificate
};
use log::error;
use parking_lot::Mutex;
use reflexo_typst::package::{DummyNotifier, Notifier, PackageError, PackageSpec, PackageRegistry};

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

    /// Path to CA certificate file for network access, especially for downloading typst packages.
    #[clap(
        long = "cert",
        env = "TYPST_CERT",
        value_name = "CERT_PATH"
    )]
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

        LspUniverseBuilder::build(entry, Arc::new(fonts), Arc::new(Prehashed::new(inputs)), cert_path)
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
pub type ImmutDict = Arc<Prehashed<TypstDict>>;

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

/// Compiler feature for LSP universe and worlds without typst.ts to implement more for tinymist.
/// type trait of [`TypstSystemWorld`].
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

/// The http registry without typst.ts to implement more for tinymist.
pub struct HttpsRegistry {
    notifier: Arc<Mutex<dyn Notifier + Send>>,

    packages: OnceLock<Vec<(PackageSpec, Option<EcoString>)>>,

    cert_path: Option<PathBuf>,
}

impl Default for HttpsRegistry {
    fn default() -> Self {
        Self { 
            notifier: Arc::new(Mutex::<DummyNotifier>::default()), 

            // todo: reset cache
            packages: OnceLock::new(), 

            // Default to None
            cert_path: None,
        }
    }
}

impl HttpsRegistry {
    fn new(cert_path: Option<PathBuf>) -> Self {
        Self { 
            notifier: Arc::new(Mutex::<DummyNotifier>::default()), 
            packages: OnceLock::new(), 
            cert_path,
        }
    }

    /// Get local path option 
    pub fn local_path(&self) -> Option<Box<Path>> {
        if let Some(data_dir) = dirs::data_dir() {
            if data_dir.exists() {
                return Some(data_dir.join("typst/packages").into());
            }
        }

        None
    }

    /// Get data & cache dir
    pub fn paths(&self) -> Vec<Box<Path>> {
        let mut res = vec![];
        if let Some(data_dir) = dirs::data_dir() {
            let dir: Box<Path> = data_dir.join("typst/packages").into();
            if dir.exists() {
                res.push(dir);
            }
        }

        if let Some(cache_dir) = dirs::cache_dir() {
            let dir: Box<Path> = cache_dir.join("typst/packages").into();
            if dir.exists() {
                res.push(dir);
            }
        }

        res
    }


    /// Make a package available in the on-disk cache.
    pub fn prepare_package(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        let subdir = format!(
            "typst/packages/{}/{}/{}",
            spec.namespace, spec.name, spec.version
        );

        if let Some(data_dir) = dirs::data_dir() {
            let dir = data_dir.join(&subdir);
            if dir.exists() {
                return Ok(dir.into());
            }
        }

        if let Some(cache_dir) = dirs::cache_dir() {
            let dir = cache_dir.join(&subdir);

            // Download from network if it doesn't exist yet.
            if spec.namespace == "preview" && !dir.exists() {
                self.download_package(spec, &dir)?;
            }

            if dir.exists() {
                return Ok(dir.into());
            }
        }

        Err(PackageError::NotFound(spec.clone()))
    }

    /// Download a package over the network.
    fn download_package(&self, spec: &PackageSpec, package_dir: &Path) -> Result<(), PackageError> {
        let url = format!(
            "https://packages.typst.org/preview/{}-{}.tar.gz",
            spec.name, spec.version
        );

        self.notifier.lock().downloading(spec);
        threaded_http(&url, |resp| {
            let reader = match resp.and_then(|r| r.error_for_status()) {
                Ok(response) => response,
                Err(err) if matches!(err.status().map(|s| s.as_u16()), Some(404)) => {
                    return Err(PackageError::NotFound(spec.clone()))
                }
                Err(err) => return Err(PackageError::NetworkFailed(Some(eco_format!("{err}")))),
            };

            let decompressed = flate2::read::GzDecoder::new(reader);
            tar::Archive::new(decompressed)
                .unpack(package_dir)
                .map_err(|err| {
                    std::fs::remove_dir_all(package_dir).ok();
                    PackageError::MalformedArchive(Some(eco_format!("{err}")))
                })
        }, self.cert_path.as_deref())
        .ok_or_else(|| PackageError::Other(Some(eco_format!("cannot spawn http thread"))))?
    }
}

impl PackageRegistry for HttpsRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<std::sync::Arc<Path>, PackageError> {
        self.prepare_package(spec)
    }

    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.packages.get_or_init(|| {
            let url = "https://packages.typst.org/preview/index.json";

            threaded_http(url, |resp| {
                let reader = match resp.and_then(|r| r.error_for_status()) {
                    Ok(response) => response,
                    Err(err) => {
                        // todo: silent error
                        error!("Failed to fetch package index: {} from {}", err, url);
                        return vec![];
                    }
                };

                #[derive(serde::Deserialize)]
                struct RemotePackageIndex {
                    name: EcoString,
                    version: PackageVersion,
                    description: Option<EcoString>,
                }

                let index: Vec<RemotePackageIndex> = match serde_json::from_reader(reader) {
                    Ok(index) => index,
                    Err(err) => {
                        error!("Failed to parse package index: {} from {}", err, url);
                        return vec![];
                    }
                };

                index
                    .into_iter()
                    .map(|e| {
                        (
                            PackageSpec {
                                namespace: "preview".into(),
                                name: e.name,
                                version: e.version,
                            },
                            e.description,
                        )
                    })
                    .collect::<Vec<_>>()
            }, self.cert_path.as_deref())
            .unwrap_or_default()
        })
    }
}

fn threaded_http<T: Send + Sync>(
    url: &str,
    f: impl FnOnce(Result<Response, reqwest::Error>) -> T + Send + Sync,
    cert_path: Option<&Path>
) -> Option<T> {
    std::thread::scope(|s| {
                s.spawn(move || {
            let client_builder = reqwest::blocking::Client::builder();

            let client = if let Some(cert_path) = cert_path {
                let cert = std::fs::read(cert_path).ok().and_then(|buf| Certificate::from_pem(&buf).ok());
                if let Some(cert) = cert {
                    client_builder.add_root_certificate(cert).build().unwrap()
                } else {
                    client_builder.build().unwrap()
                }
            } else {
                client_builder.build().unwrap()
            };

            f(client.get(url).send())
        })
        .join()
        .ok()
    })
}
