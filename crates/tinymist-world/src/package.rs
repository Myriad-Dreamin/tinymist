//! Https registry for tinymist.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use reflexo_typst::package::{DummyNotifier, Notifier, PackageError, PackageRegistry, PackageSpec};
use reflexo_typst::typst::diag::EcoString;
use reflexo_typst::ImmutPath;
use reqwest::blocking::Response;
use reqwest::Certificate;
use typst::diag::{eco_format, PackageResult, StrResult};
use typst::syntax::package::{PackageVersion, VersionlessPackageSpec};

use crate::CompilePackageArgs;

/// The https package registry for tinymist.
pub struct HttpsRegistry {
    /// The path at which local packages (`@local` packages) are stored.
    local_dir: Option<ImmutPath>,
    /// The path at which non-local packages (`@preview` packages) should be
    /// stored when downloaded.
    cache_dir: Option<ImmutPath>,
    /// lazily initialized package storage.
    storage: OnceLock<PackageStorage>,
    /// The path to the certificate file to use for HTTPS requests.
    cert_path: Option<ImmutPath>,
    /// The notifier to use for progress updates.
    notifier: Arc<Mutex<dyn Notifier + Send>>,
    // package_dir_cache: RwLock<HashMap<PackageSpec, Result<ImmutPath, PackageError>>>,
}

impl Default for HttpsRegistry {
    fn default() -> Self {
        Self {
            notifier: Arc::new(Mutex::<DummyNotifier>::default()),
            cert_path: None,
            local_dir: None,
            cache_dir: None,

            storage: OnceLock::new(),
            // package_dir_cache: RwLock::new(HashMap::new()),
        }
    }
}

impl std::ops::Deref for HttpsRegistry {
    type Target = PackageStorage;

    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}

impl HttpsRegistry {
    /// Create a new registry.
    pub fn new(cert_path: Option<ImmutPath>, args: Option<&CompilePackageArgs>) -> Self {
        Self {
            cert_path,
            local_dir: args.and_then(|args| Some(args.package_path.as_deref()?.into())),
            cache_dir: args.and_then(|args| Some(args.package_cache_path.as_deref()?.into())),
            ..Default::default()
        }
    }

    /// Get local path option
    pub fn local_path(&self) -> Option<ImmutPath> {
        self.storage().package_path().cloned()
    }

    /// Get data & cache dir
    pub fn paths(&self) -> Vec<ImmutPath> {
        let data_dir = self.storage().package_path().cloned();
        let cache_dir = self.storage().package_cache_path().cloned();
        data_dir.into_iter().chain(cache_dir).collect::<Vec<_>>()
    }

    /// Get `typst-kit` implementing package storage
    pub fn storage(&self) -> &PackageStorage {
        self.storage.get_or_init(|| {
            PackageStorage::new(
                self.cache_dir
                    .clone()
                    .or_else(|| Some(dirs::cache_dir()?.join(DEFAULT_PACKAGES_SUBDIR).into())),
                self.local_dir
                    .clone()
                    .or_else(|| Some(dirs::data_dir()?.join(DEFAULT_PACKAGES_SUBDIR).into())),
                self.cert_path.clone(),
                self.notifier.clone(),
            )
        })
    }

    /// Make a package available in the on-disk cache.
    pub fn prepare_package(&self, spec: &PackageSpec) -> Result<ImmutPath, PackageError> {
        self.storage().prepare_package(spec)
    }
}

impl PackageRegistry for HttpsRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<ImmutPath, PackageError> {
        self.prepare_package(spec)
    }

    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.storage().download_index()
    }
}

/// The default Typst registry.
pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";

/// The default packages sub directory within the package and package cache
/// paths.
pub const DEFAULT_PACKAGES_SUBDIR: &str = "typst/packages";

/// Holds information about where packages should be stored and downloads them
/// on demand, if possible.
pub struct PackageStorage {
    /// The path at which non-local packages should be stored when downloaded.
    package_cache_path: Option<ImmutPath>,
    /// The path at which local packages are stored.
    package_path: Option<ImmutPath>,
    /// The downloader used for fetching the index and packages.
    cert_path: Option<ImmutPath>,
    /// The cached index of the preview namespace.
    index: OnceLock<Vec<(PackageSpec, Option<EcoString>)>>,
    notifier: Arc<Mutex<dyn Notifier + Send>>,
}

impl PackageStorage {
    /// Creates a new package storage for the given package paths.
    /// It doesn't fallback directories, thus you can disable the related
    /// storage by passing `None`.
    pub fn new(
        package_cache_path: Option<ImmutPath>,
        package_path: Option<ImmutPath>,
        cert_path: Option<ImmutPath>,
        notifier: Arc<Mutex<dyn Notifier + Send>>,
    ) -> Self {
        Self {
            package_cache_path,
            package_path,
            cert_path,
            notifier,
            index: OnceLock::new(),
        }
    }

    /// Returns the path at which non-local packages should be stored when
    /// downloaded.
    pub fn package_cache_path(&self) -> Option<&ImmutPath> {
        self.package_cache_path.as_ref()
    }

    /// Returns the path at which local packages are stored.
    pub fn package_path(&self) -> Option<&ImmutPath> {
        self.package_path.as_ref()
    }

    /// Make a package available in the on-disk.
    pub fn prepare_package(&self, spec: &PackageSpec) -> PackageResult<ImmutPath> {
        let subdir = format!("{}/{}/{}", spec.namespace, spec.name, spec.version);

        if let Some(packages_dir) = &self.package_path {
            let dir = packages_dir.join(&subdir);
            if dir.exists() {
                return Ok(dir.into());
            }
        }

        if let Some(cache_dir) = &self.package_cache_path {
            let dir = cache_dir.join(&subdir);
            if dir.exists() {
                return Ok(dir.into());
            }

            // Download from network if it doesn't exist yet.
            if spec.namespace == "preview" {
                self.download_package(spec, &dir)?;
                if dir.exists() {
                    return Ok(dir.into());
                }
            }
        }

        Err(PackageError::NotFound(spec.clone()))
    }

    /// Try to determine the latest version of a package.
    pub fn determine_latest_version(
        &self,
        spec: &VersionlessPackageSpec,
    ) -> StrResult<PackageVersion> {
        if spec.namespace == "preview" {
            // For `@preview`, download the package index and find the latest
            // version.
            self.download_index()
                .iter()
                .filter(|(package, _)| package.name == spec.name)
                .map(|(package, _)| package.version)
                .max()
                .ok_or_else(|| eco_format!("failed to find package {spec}"))
        } else {
            // For other namespaces, search locally. We only search in the data
            // directory and not the cache directory, because the latter is not
            // intended for storage of local packages.
            let subdir = format!("{}/{}", spec.namespace, spec.name);
            self.package_path
                .iter()
                .flat_map(|dir| std::fs::read_dir(dir.join(&subdir)).ok())
                .flatten()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter_map(|path| path.file_name()?.to_string_lossy().parse().ok())
                .max()
                .ok_or_else(|| eco_format!("please specify the desired version"))
        }
    }

    /// Download the package index. The result of this is cached for efficiency.
    pub fn download_index(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.index.get_or_init(|| {
            let url = format!("{DEFAULT_REGISTRY}/preview/index.json");

            threaded_http(&url, self.cert_path.as_deref(), |resp| {
                let reader = match resp.and_then(|r| r.error_for_status()) {
                    Ok(response) => response,
                    Err(err) => {
                        // todo: silent error
                        log::error!("Failed to fetch package index: {err} from {url}");
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
                        log::error!("Failed to parse package index: {err} from {url}");
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
            })
            .unwrap_or_default()
        })
    }

    /// Download a package over the network.
    ///
    /// # Panics
    /// Panics if the package spec namespace isn't `preview`.
    pub fn download_package(&self, spec: &PackageSpec, package_dir: &Path) -> PackageResult<()> {
        assert_eq!(spec.namespace, "preview");

        let url = format!(
            "{DEFAULT_REGISTRY}/preview/{}-{}.tar.gz",
            spec.name, spec.version
        );

        self.notifier.lock().downloading(spec);
        threaded_http(&url, self.cert_path.as_deref(), |resp| {
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
        })
        .ok_or_else(|| PackageError::Other(Some(eco_format!("cannot spawn http thread"))))?
    }
}

fn threaded_http<T: Send + Sync>(
    url: &str,
    cert_path: Option<&Path>,
    f: impl FnOnce(Result<Response, reqwest::Error>) -> T + Send + Sync,
) -> Option<T> {
    std::thread::scope(|s| {
        s.spawn(move || {
            let client_builder = reqwest::blocking::Client::builder();

            let client = if let Some(cert_path) = cert_path {
                let cert = std::fs::read(cert_path)
                    .ok()
                    .and_then(|buf| Certificate::from_pem(&buf).ok());
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
