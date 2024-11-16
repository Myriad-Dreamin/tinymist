//! Https registry for tinymist.

use std::sync::OnceLock;
use std::{path::PathBuf, sync::Arc};

use parking_lot::Mutex;
use reflexo_typst::package::{DummyNotifier, Notifier, PackageError, PackageRegistry, PackageSpec};
use reflexo_typst::typst::diag::EcoString;
use reflexo_typst::ImmutPath;
use typst_kit::download::{DownloadState, Downloader};
use typst_kit::package::PackageStorage;

/// The https package registry for tinymist.
pub struct HttpsRegistry {
    /// The path at which local packages (`@local` packages) are stored.
    local_dir: OnceLock<Option<ImmutPath>>,
    /// The path at which non-local packages (`@preview` packages) should be
    /// stored when downloaded.
    cache_dir: OnceLock<Option<ImmutPath>>,
    /// The cached index of the preview namespace.
    index: OnceLock<Vec<(PackageSpec, Option<EcoString>)>>,
    /// lazily initialized package storage.
    storage: OnceLock<PackageStorage>,
    cert_path: Option<PathBuf>,
    notifier: Arc<Mutex<dyn Notifier + Send>>,
    // package_dir_cache: RwLock<HashMap<PackageSpec, Result<ImmutPath, PackageError>>>,
}

impl Default for HttpsRegistry {
    fn default() -> Self {
        Self {
            notifier: Arc::new(Mutex::<DummyNotifier>::default()),
            // todo: reset cache
            index: OnceLock::new(),
            // Default to None
            cert_path: None,

            local_dir: OnceLock::new(),
            cache_dir: OnceLock::new(),
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
    pub fn new(cert_path: Option<PathBuf>) -> Self {
        Self {
            cert_path,
            ..Default::default()
        }
    }

    /// Get local path option
    pub fn local_path(&self) -> Option<ImmutPath> {
        self.data_dir().cloned()
    }

    fn data_dir(&self) -> Option<&ImmutPath> {
        self.local_dir
            .get_or_init(|| Some(dirs::data_dir()?.join("typst/packages").into()))
            .as_ref()
    }

    fn cache_dir(&self) -> Option<&ImmutPath> {
        self.cache_dir
            .get_or_init(|| Some(dirs::cache_dir()?.join("typst/packages").into()))
            .as_ref()
    }

    /// Get data & cache dir
    pub fn paths(&self) -> Vec<ImmutPath> {
        let mut res = Vec::with_capacity(2);
        if let Some(data_dir) = self.data_dir() {
            res.push(data_dir.clone());
        }

        if let Some(cache_dir) = self.cache_dir() {
            res.push(cache_dir.clone())
        }

        res
    }

    /// Get `typst-kit` implementing package storage
    pub fn storage(&self) -> &PackageStorage {
        self.storage.get_or_init(|| {
            let user_agent = concat!("typst/", env!("CARGO_PKG_VERSION"));
            let downloader = match self.cert_path.clone() {
                Some(cert) => Downloader::with_path(user_agent, cert),
                None => Downloader::new(user_agent),
            };
            PackageStorage::new(
                self.cache_dir().map(|s| s.as_ref().into()),
                self.data_dir().map(|s| s.as_ref().into()),
                downloader,
            )
        })
    }

    /// Make a package available in the on-disk cache.
    pub fn prepare_package(&self, spec: &PackageSpec) -> Result<PathBuf, PackageError> {
        self.storage()
            .prepare_package(spec, &mut NotifierProgress(self.notifier.clone(), spec))
    }
}

impl PackageRegistry for HttpsRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<ImmutPath, PackageError> {
        self.prepare_package(spec).map(From::from)
    }

    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.index.get_or_init(|| {
            let packages = self.storage().download_index().unwrap_or_default().iter();

            packages
                .map(|e| {
                    (
                        PackageSpec {
                            namespace: "preview".into(),
                            name: e.name.clone(),
                            version: e.version,
                        },
                        e.description.clone(),
                    )
                })
                .collect()
        })
    }
}

struct NotifierProgress<'a>(Arc<Mutex<dyn Notifier + Send>>, &'a PackageSpec);

impl typst_kit::download::Progress for NotifierProgress<'_> {
    fn print_start(&mut self) {
        self.0.lock().downloading(self.1);
    }
    fn print_progress(&mut self, _state: &DownloadState) {}
    fn print_finish(&mut self, _state: &DownloadState) {}
}
