//! Https registry for tinymist.

use std::path::Path;
use std::sync::OnceLock;
use std::{path::PathBuf, sync::Arc};

use log::error;
use parking_lot::Mutex;
use reflexo_typst::package::{DummyNotifier, Notifier, PackageError, PackageRegistry, PackageSpec};
use reflexo_typst::typst::{
    diag::{eco_format, EcoString},
    syntax::package::PackageVersion,
};
use reqwest::{blocking::Response, Certificate};

/// The http registry without typst.ts to implement more for tinymist.
pub struct HttpsRegistry {
    notifier: Arc<Mutex<dyn Notifier + Send>>,

    packages: OnceLock<Vec<(PackageSpec, Option<EcoString>)>>,

    cert_path: Option<PathBuf>,

    data_dir_cache: OnceLock<Option<Arc<Path>>>,
    cache_dir_cache: OnceLock<Option<Arc<Path>>>,
    // package_dir_cache: RwLock<HashMap<PackageSpec, Result<Arc<Path>, PackageError>>>,
}

impl Default for HttpsRegistry {
    fn default() -> Self {
        Self {
            notifier: Arc::new(Mutex::<DummyNotifier>::default()),

            // todo: reset cache
            packages: OnceLock::new(),

            // Default to None
            cert_path: None,

            data_dir_cache: OnceLock::new(),
            cache_dir_cache: OnceLock::new(),
            // package_dir_cache: RwLock::new(HashMap::new()),
        }
    }
}

impl HttpsRegistry {
    /// Create a new registry.
    pub fn new(cert_path: Option<PathBuf>) -> Self {
        Self {
            cert_path,
            ..Default::default()
        }
    }

    /// Get local path option
    pub fn local_path(&self) -> Option<Box<Path>> {
        if let Some(data_dir) = self.data_dir() {
            if data_dir.exists() {
                return Some(data_dir.join("typst/packages").into());
            }
        }

        None
    }

    fn data_dir(&self) -> Option<&Arc<Path>> {
        self.data_dir_cache
            .get_or_init(|| dirs::data_dir().map(From::from))
            .as_ref()
    }

    fn cache_dir(&self) -> Option<&Arc<Path>> {
        self.cache_dir_cache
            .get_or_init(|| dirs::cache_dir().map(From::from))
            .as_ref()
    }

    /// Get data & cache dir
    pub fn paths(&self) -> Vec<Box<Path>> {
        let mut res = vec![];
        if let Some(data_dir) = self.data_dir() {
            let dir: Box<Path> = data_dir.join("typst/packages").into();
            if dir.exists() {
                res.push(dir);
            }
        }

        if let Some(cache_dir) = self.cache_dir() {
            let dir: Box<Path> = cache_dir.join("typst/packages").into();
            if dir.exists() {
                res.push(dir);
            }
        }

        res
    }

    /// Make a package available in the on-disk cache.
    pub fn prepare_package(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        // let cache = self.package_dir_cache.read();
        // if let Some(dir) = cache.get(spec) {
        //     return dir.clone();
        // }

        // drop(cache);
        // let mut cache = self.package_dir_cache.write();
        // if let Some(dir) = cache.get(spec) {
        //     return dir.clone();
        // }

        // let dir = self.prepare_package_(spec);
        // cache.insert(spec.clone(), dir.clone());
        // dir
        self.prepare_package_(spec)
    }

    /// Make a package available in the on-disk cache.
    pub fn prepare_package_(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        let subdir = format!(
            "typst/packages/{}/{}/{}",
            spec.namespace, spec.name, spec.version
        );

        if let Some(data_dir) = self.data_dir() {
            let dir = data_dir.join(&subdir);
            if dir.exists() {
                return Ok(dir.into());
            }
        }

        if let Some(cache_dir) = self.cache_dir() {
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

impl PackageRegistry for HttpsRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<std::sync::Arc<Path>, PackageError> {
        self.prepare_package(spec)
    }

    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.packages.get_or_init(|| {
            let url = "https://packages.typst.org/preview/index.json";

            threaded_http(url, self.cert_path.as_deref(), |resp| {
                let reader = match resp.and_then(|r| r.error_for_status()) {
                    Ok(response) => response,
                    Err(err) => {
                        // todo: silent error
                        error!("Failed to fetch package index: {err} from {url}");
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
                        error!("Failed to parse package index: {err} from {url}");
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
