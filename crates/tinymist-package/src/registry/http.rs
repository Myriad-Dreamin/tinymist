//! Http registry for tinymist.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use reqwest::Certificate;
use reqwest::blocking::Response;
use tinymist_std::ImmutPath;
use typst::diag::{PackageResult, StrResult, eco_format};
use typst::syntax::package::{PackageVersion, VersionlessPackageSpec};

use crate::registry::{PREVIEW_NS, PackageIndexEntry, PackageSpecExt};

use super::{
    DEFAULT_REGISTRY, DummyNotifier, Notifier, PackageError, PackageRegistry, PackageSpec,
};

/// The http package registry for typst.ts.
pub struct HttpRegistry {
    /// The path at which local packages (`@local` packages) are stored.
    package_path: Option<ImmutPath>,
    /// The path at which non-local packages (`@preview` packages) should be
    /// stored when downloaded.
    package_cache_path: Option<ImmutPath>,
    /// lazily initialized package storage.
    storage: OnceLock<PackageStorage>,
    /// The path to the certificate file to use for HTTPS requests.
    cert_path: Option<ImmutPath>,
    /// The notifier to use for progress updates.
    notifier: Arc<Mutex<dyn Notifier + Send>>,
    // package_dir_cache: RwLock<HashMap<PackageSpec, Result<ImmutPath, PackageError>>>,
}

impl Default for HttpRegistry {
    fn default() -> Self {
        Self {
            notifier: Arc::new(Mutex::<DummyNotifier>::default()),
            cert_path: None,
            package_path: None,
            package_cache_path: None,

            storage: OnceLock::new(),
            // package_dir_cache: RwLock::new(HashMap::new()),
        }
    }
}

impl std::ops::Deref for HttpRegistry {
    type Target = PackageStorage;

    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}

impl HttpRegistry {
    /// Create a new registry.
    pub fn new(
        cert_path: Option<ImmutPath>,
        package_path: Option<ImmutPath>,
        package_cache_path: Option<ImmutPath>,
    ) -> Self {
        Self {
            cert_path,
            package_path,
            package_cache_path,
            ..Default::default()
        }
    }

    /// Get `typst-kit` implementing package storage
    pub fn storage(&self) -> &PackageStorage {
        self.storage.get_or_init(|| {
            PackageStorage::new(
                self.package_cache_path
                    .clone()
                    .or_else(|| Some(dirs::cache_dir()?.join(DEFAULT_PACKAGES_SUBDIR).into())),
                self.package_path
                    .clone()
                    .or_else(|| Some(dirs::data_dir()?.join(DEFAULT_PACKAGES_SUBDIR).into())),
                self.cert_path.clone(),
                self.notifier.clone(),
            )
        })
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

    /// Set list of packages for testing.
    pub fn test_package_list(&self, f: impl FnOnce() -> Vec<PackageIndexEntry>) {
        self.storage().index.get_or_init(f);
    }
}

impl PackageRegistry for HttpRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<ImmutPath, PackageError> {
        self.storage().prepare_package(spec)
    }

    fn packages(&self) -> &[PackageIndexEntry] {
        self.storage().download_index()
    }
}

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
    index: OnceLock<Vec<PackageIndexEntry>>,
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

    /// Make a package available in the on-disk cache.
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
            if spec.is_preview() {
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
        if spec.is_preview() {
            // For `@preview`, download the package index and find the latest
            // version.
            self.download_index()
                .iter()
                .filter(|entry| entry.package.name == spec.name)
                .map(|entry| entry.package.version)
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

    /// Get the cached package index without network access.
    pub fn cached_index(&self) -> Option<&[PackageIndexEntry]> {
        self.index.get().map(Vec::as_slice)
    }

    /// Download the package index. The result of this is cached for efficiency.
    pub fn download_index(&self) -> &[PackageIndexEntry] {
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

                let mut entries: Vec<PackageIndexEntry> = match serde_json::from_reader(reader) {
                    Ok(entry) => entry,
                    Err(err) => {
                        log::error!("Failed to parse package index: {err} from {url}");
                        return vec![];
                    }
                };
                for entry in &mut entries {
                    entry.namespace = PREVIEW_NS.into();
                }

                entries
            })
            .unwrap_or_default()
        })
    }

    /// Download a package over the network.
    ///
    /// # Panics
    /// Panics if the package spec namespace isn't `preview`.
    pub fn download_package(&self, spec: &PackageSpec, package_dir: &Path) -> PackageResult<()> {
        assert!(spec.is_preview(), "only preview packages can be downloaded");

        let url = format!(
            "{DEFAULT_REGISTRY}/preview/{}-{}.tar.gz",
            spec.name, spec.version
        );

        self.notifier.lock().downloading(spec);
        threaded_http(&url, self.cert_path.as_deref(), |resp| {
            let reader = match resp.and_then(|r| r.error_for_status()) {
                Ok(response) => response,
                Err(err) if matches!(err.status().map(|s| s.as_u16()), Some(404)) => {
                    return Err(PackageError::NotFound(spec.clone()));
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

pub(crate) fn threaded_http<T: Send + Sync>(
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use super::*;

    fn drawmatrix_spec() -> PackageSpec {
        PackageSpec::from_str("@local/drawmatrix:0.1.0").expect("valid package spec")
    }

    fn write_package_dir(root: &Path) -> PathBuf {
        let package_dir = root.join("local/drawmatrix/0.1.0");
        std::fs::create_dir_all(&package_dir).expect("package directory should be created");
        package_dir
    }

    fn storage(data_root: &Path, cache_root: &Path) -> PackageStorage {
        PackageStorage::new(
            Some(cache_root.into()),
            Some(data_root.into()),
            None,
            Arc::new(Mutex::<DummyNotifier>::default()),
        )
    }

    #[test]
    fn package_storage_resolves_data_then_cache_for_local_packages() {
        struct Case {
            name: &'static str,
            data_package_exists: bool,
            cache_package_exists: bool,
            expected_root: Option<&'static str>,
        }

        let temp = tempfile::tempdir().expect("tempdir should be created");
        let spec = drawmatrix_spec();

        for (idx, case) in [
            Case {
                name: "data only",
                data_package_exists: true,
                cache_package_exists: false,
                expected_root: Some("data"),
            },
            Case {
                name: "cache only",
                data_package_exists: false,
                cache_package_exists: true,
                expected_root: Some("cache"),
            },
            Case {
                name: "data and cache",
                data_package_exists: true,
                cache_package_exists: true,
                expected_root: Some("data"),
            },
            Case {
                name: "neither data nor cache",
                data_package_exists: false,
                cache_package_exists: false,
                expected_root: None,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let case_root = temp.path().join(format!("case-{idx}"));
            let data_root = case_root.join("data");
            let cache_root = case_root.join("cache");

            if case.data_package_exists {
                write_package_dir(&data_root);
            }

            if case.cache_package_exists {
                write_package_dir(&cache_root);
            }

            let storage = storage(&data_root, &cache_root);
            let result = storage.prepare_package(&spec);

            match case.expected_root {
                Some("data") => {
                    let resolved = result.unwrap_or_else(|err| {
                        panic!(
                            "expected data package to resolve for {}, got {err:?}",
                            case.name
                        )
                    });
                    assert_eq!(resolved.as_ref(), data_root.join("local/drawmatrix/0.1.0"));
                }
                Some("cache") => {
                    let resolved = result.unwrap_or_else(|err| {
                        panic!(
                            "expected cache package to resolve for {}, got {err:?}",
                            case.name
                        )
                    });
                    assert_eq!(resolved.as_ref(), cache_root.join("local/drawmatrix/0.1.0"));
                }
                None => {
                    assert!(
                        matches!(result, Err(PackageError::NotFound(_))),
                        "expected missing package for {}, got {result:?}",
                        case.name
                    );
                }
                Some(other) => unreachable!("unexpected expected root {other}"),
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn default_linux_package_dirs_follow_process_xdg_environment() {
        use std::ffi::OsStr;

        let temp = tempfile::tempdir().expect("tempdir should be created");
        let fallback_home = temp.path().join("fallback-home");
        std::fs::create_dir_all(&fallback_home).expect("fallback home directory should be created");

        let env: [(&str, Option<&OsStr>); 5] = [
            ("HOME", Some(fallback_home.as_os_str())),
            ("XDG_DATA_HOME", None),
            ("XDG_CACHE_HOME", None),
            ("TYPST_PACKAGE_PATH", None),
            ("TYPST_PACKAGE_CACHE_PATH", None),
        ];

        temp_env::with_vars(env, || {
            let registry = HttpRegistry::default();
            let storage = registry.storage();

            let expected_data = fallback_home.join(".local/share/typst/packages");
            let expected_cache = fallback_home.join(".cache/typst/packages");
            assert_eq!(
                storage.package_path().map(|path| path.as_ref()),
                Some(expected_data.as_path())
            );
            assert_eq!(
                storage.package_cache_path().map(|path| path.as_ref()),
                Some(expected_cache.as_path())
            );

            let expected = write_package_dir(&expected_data);
            let resolved = storage
                .prepare_package(&drawmatrix_spec())
                .expect("package under HOME fallback should resolve");
            assert_eq!(resolved.as_ref(), expected);
        });

        let home = temp.path().join("xdg-home");
        let xdg_data_home = temp.path().join("xdg-data");
        let xdg_cache_home = temp.path().join("xdg-cache");
        std::fs::create_dir_all(&home).expect("XDG home directory should be created");

        let env: [(&str, Option<&OsStr>); 5] = [
            ("HOME", Some(home.as_os_str())),
            ("XDG_DATA_HOME", Some(xdg_data_home.as_os_str())),
            ("XDG_CACHE_HOME", Some(xdg_cache_home.as_os_str())),
            ("TYPST_PACKAGE_PATH", None),
            ("TYPST_PACKAGE_CACHE_PATH", None),
        ];

        temp_env::with_vars(env, || {
            let registry = HttpRegistry::default();
            let storage = registry.storage();

            assert_eq!(
                storage.package_path().map(|path| path.as_ref()),
                Some(xdg_data_home.join(DEFAULT_PACKAGES_SUBDIR).as_path())
            );
            assert_eq!(
                storage.package_cache_path().map(|path| path.as_ref()),
                Some(xdg_cache_home.join(DEFAULT_PACKAGES_SUBDIR).as_path())
            );

            let spec = drawmatrix_spec();
            write_package_dir(&home.join(".local/share/typst/packages"));
            let result = storage.prepare_package(&spec);
            assert!(
                matches!(result, Err(PackageError::NotFound(_))),
                "HOME fallback should not be searched while XDG_DATA_HOME is set, got {result:?}"
            );

            let expected = write_package_dir(&xdg_data_home.join(DEFAULT_PACKAGES_SUBDIR));
            let resolved = storage
                .prepare_package(&spec)
                .expect("package under XDG_DATA_HOME should resolve");
            assert_eq!(resolved.as_ref(), expected);
        });
    }
}
