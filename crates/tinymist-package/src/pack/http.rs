use ecow::eco_format;
use typst::diag::PackageError;

use super::*;
use crate::registry::threaded_http;

/// A package in the remote http.
#[derive(Clone)]
pub struct HttpPack<S> {
    /// The package specifier.
    pub specifier: PackageSpec,
    /// The url of the package.
    pub url: S,
}

impl<S: AsRef<str>> HttpPack<S> {
    /// Creates a new `HttpPack` instance.
    pub fn new(specifier: PackageSpec, url: S) -> Self {
        Self { specifier, url }
    }
}

impl<S: AsRef<str>> fmt::Debug for HttpPack<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HttpPack({})", self.url.as_ref())
    }
}

impl<S: AsRef<str>> PackFs for HttpPack<S> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        let spec = &self.specifier;
        let url = self.url.as_ref();
        threaded_http(url, None, |resp| {
            let reader = match resp.and_then(|r| r.error_for_status()) {
                Ok(response) => response,
                Err(err) if matches!(err.status().map(|s| s.as_u16()), Some(404)) => {
                    return Err(PackageError::NotFound(spec.clone()))
                }
                Err(err) => return Err(PackageError::NetworkFailed(Some(eco_format!("{err}")))),
            };

            let decompressed = flate2::read::GzDecoder::new(reader);
            let mut tarbar = TarballPack::new(decompressed);

            // .unpack(package_dir)
            // .map_err(|err| {
            //     std::fs::remove_dir_all(package_dir).ok();
            //     PackageError::MalformedArchive(Some(eco_format!("{err}")))
            // })

            tarbar.read_all(f)
        })
        .ok_or_else(|| PackageError::Other(Some(eco_format!("cannot spawn http thread"))))?
    }
}

impl<S: AsRef<str>> Pack for HttpPack<S> {}
