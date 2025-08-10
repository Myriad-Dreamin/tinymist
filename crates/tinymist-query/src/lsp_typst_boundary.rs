//! Conversions between Typst and LSP types and representations

use tinymist_std::path::PathClean;
use tinymist_world::vfs::PathResolution;

use crate::prelude::*;

/// An LSP Position encoded by [`PositionEncoding`].
pub use tinymist_analysis::location::LspPosition;
/// An LSP range encoded by [`PositionEncoding`].
pub use tinymist_analysis::location::LspRange;

pub use tinymist_analysis::location::*;

const UNTITLED_ROOT: &str = "/untitled";
static EMPTY_URL: LazyLock<Url> = LazyLock::new(|| Url::parse("file://").unwrap());

/// Convert a path to a URL.
pub fn untitled_url(path: &Path) -> anyhow::Result<Url> {
    Ok(Url::parse(&format!("untitled:{}", path.display()))?)
}

/// Convert a path to a URL.
pub fn path_to_url(path: &Path) -> anyhow::Result<Url> {
    if let Ok(untitled) = path.strip_prefix(UNTITLED_ROOT) {
        // rust-url will panic on converting an empty path.
        if untitled == Path::new("nEoViM-BuG") {
            return Ok(EMPTY_URL.clone());
        }

        return untitled_url(untitled);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Url::from_file_path(path).or_else(|never| {
            let _: () = never;

            anyhow::bail!("could not convert path to URI: path: {path:?}",)
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        // In WASM, create a simple file:// URL
        let path_str = path.to_string_lossy();
        let url_str = if path_str.starts_with('/') {
            format!("file://{}", path_str)
        } else {
            format!("file:///{}", path_str)
        };
        Url::parse(&url_str).map_err(|e| anyhow::anyhow!("could not convert path to URI: {}", e))
    }
}

/// Convert a path resolution to a URL.
pub fn path_res_to_url(path: PathResolution) -> anyhow::Result<Url> {
    match path {
        PathResolution::Rootless(path) => untitled_url(path.as_rooted_path()),
        PathResolution::Resolved(path) => path_to_url(&path),
    }
}

/// Convert a URL to a path.
pub fn url_to_path(uri: Url) -> PathBuf {
    if uri.scheme() == "file" {
        // typst converts an empty path to `Path::new("/")`, which is undesirable.
        if !uri.has_host() && uri.path() == "/" {
            return PathBuf::from("/untitled/nEoViM-BuG");
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            return uri
                .to_file_path()
                .unwrap_or_else(|_| panic!("could not convert URI to path: URI: {uri:?}",));
        }
        #[cfg(target_arch = "wasm32")]
        {
            // In WASM, manually parse the file:// URL
            let path = uri.path();
            return PathBuf::from(path);
        }
    }

    if uri.scheme() == "untitled" {
        let mut bytes = UNTITLED_ROOT.as_bytes().to_vec();

        // This is rust-url's path_segments, but vscode's untitle doesn't like it.
        let path = uri.path();
        let segs = path.strip_prefix('/').unwrap_or(path).split('/');
        for segment in segs {
            bytes.push(b'/');
            bytes.extend(percent_encoding::percent_decode(segment.as_bytes()));
        }

        return Path::new(String::from_utf8_lossy(&bytes).as_ref()).clean();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        uri.to_file_path().unwrap()
    }
    #[cfg(target_arch = "wasm32")]
    {
        // In WASM, manually parse the URL path
        PathBuf::from(uri.path())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_untitled() {
        let path = Path::new("/untitled/test");
        let uri = path_to_url(path).unwrap();
        assert_eq!(uri.scheme(), "untitled");
        assert_eq!(uri.path(), "test");

        let path = url_to_path(uri);
        assert_eq!(path, Path::new("/untitled/test").clean());
    }

    #[test]
    fn unnamed_buffer() {
        // https://github.com/neovim/nvim-lspconfig/pull/2226
        let uri = EMPTY_URL.clone();
        let path = url_to_path(uri);
        assert_eq!(path, Path::new("/untitled/nEoViM-BuG"));

        let uri2 = path_to_url(&path).unwrap();
        assert_eq!(EMPTY_URL.clone(), uri2);
    }
}
