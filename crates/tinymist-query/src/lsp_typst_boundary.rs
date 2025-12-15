//! Conversions between Typst and LSP types and representations

use anyhow::Context;
use percent_encoding::percent_decode_str;
use tinymist_std::path::{PathClean, unix_slash};
use tinymist_world::vfs::PathResolution;
use tinymist_std::path::looks_like_uri;

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

/// Determines if a path is untitled.
pub fn is_untitled_path(p: &Path) -> bool {
    p.starts_with(UNTITLED_ROOT)
}

/// Convert a path to a URL.
pub fn path_to_url(path: &Path) -> anyhow::Result<Url> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path_str = unix_slash(path);
        if looks_like_uri(&path_str) {
            // let `url::Url` handle percent-encoding of the path component
            // ensures that characters like spaces are encoded as `%20`
            if let Some(pos) = path_str.find(':') {
                let (scheme, rest) = path_str.split_at(pos);
                let raw_path = &rest[1..]; // skip ':'

                let mut url = Url::parse(&format!("{scheme}:"))
                    .with_context(|| {
                        format!("could not convert path to URI: path: {path:?}")
                    })?;
                url.set_path(raw_path);
                return Ok(url);
            }

            // this should never happen given `looks_like_uri`, but the old behaviour is here anyway
            return Url::parse(&path_str)
                .with_context(|| format!("could not convert path to URI: path: {path:?}"));
        }
    }

    // on windows, paths with `/untitled` prefix get normalized to backslashes
    let path_str = path.to_string_lossy();
    let backslash_prefix = UNTITLED_ROOT.replace('/', "\\");

    let is_untitled = path_str.starts_with(UNTITLED_ROOT) || path_str.starts_with(&backslash_prefix);

    if is_untitled {
        if let Ok(untitled) = path.strip_prefix(UNTITLED_ROOT) {
            // rust-url will panic on converting an empty path.
            if untitled == Path::new("nEoViM-BuG") {
                return Ok(EMPTY_URL.clone());
            }
            
            return untitled_url(untitled);
        }

        // fallback: manually extract and normalize for windows backslashes
        let trimmed = if path_str.starts_with(UNTITLED_ROOT) {
            path_str.strip_prefix(UNTITLED_ROOT).unwrap_or(&path_str)
        } else {
            path_str.strip_prefix(&backslash_prefix).unwrap_or(&path_str)
        };

        let normalized = trimmed.trim_start_matches('/').trim_start_matches('\\').replace('\\', "/");
        return untitled_url(Path::new(&normalized));
    }

    url_from_file_path(path)
}

/// Convert a path resolution to a URL.
pub fn path_res_to_url(path: PathResolution) -> anyhow::Result<Url> {
    match path {
        PathResolution::Rootless(path) => untitled_url(path.as_rooted_path()),
        PathResolution::Resolved(path) => path_to_url(&path),
    }
}

/// Convert a URL to a path.
pub fn url_to_path(uri: &Url) -> PathBuf {
    if uri.scheme() == "file" {
        // typst converts an empty path to `Path::new("/")`, which is undesirable.
        if !uri.has_host() && uri.path() == "/" {
            return PathBuf::from("/untitled/nEoViM-BuG");
        }

        return url_to_file_path(uri);
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

    // for non-file, non-untitled schemes (virtual filesystem providers), encode the scheme back into the path while decoding any percent-encoding from the URL
    // it means that filesystem paths can use the same textual representation as Typst virtual paths, which may contain characters like spaces
    let decoded_path = percent_decode_str(uri.path()).decode_utf8_lossy();
    let raw = format!("{}:{}", uri.scheme(), decoded_path);
    PathBuf::from(raw)
}

#[cfg(not(target_arch = "wasm32"))]
fn url_from_file_path(path: &Path) -> anyhow::Result<Url> {
    // Prefer `Url::from_file_path` for correctness; fall back to manual construction
    // to handle edge cases like UNC paths, leading double slashes, or drive letters.
    Url::from_file_path(path).or_else(|never| {
        let _: () = never;

        let p = path.to_string_lossy().replace('\\', "/");
        let url_str = if p.starts_with("//") {
            format!("file:{}", p)
        } else if p.starts_with('/') {
            format!("file://{}", p)
        } else {
            format!("file:///{}", p)
        };

        Url::parse(&url_str)
            .with_context(|| format!("could not convert path to URI: path: {path:?}"))
    })
}


#[cfg(target_arch = "wasm32")]
fn url_from_file_path(path: &Path) -> anyhow::Result<Url> {
    // In WASM, create a simple file:// URL
    let path_str = path.to_string_lossy();
    let url_str = if path_str.starts_with('/') {
        format!("file://{}", path_str)
    } else {
        format!("file:///{}", path_str)
    };
    Url::parse(&url_str).map_err(|e| anyhow::anyhow!("could not convert path to URI: {}", e))
}

#[cfg(not(target_arch = "wasm32"))]
fn url_to_file_path(uri: &Url) -> PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| panic!("could not convert URI to path: URI: {uri:?}",))
}

#[cfg(target_arch = "wasm32")]
fn url_to_file_path(uri: &Url) -> PathBuf {
    // In WASM, manually parse the URL path
    PathBuf::from(uri.path())
}
#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_untitled() {
        let path = Path::new("/untitled/test");
        let uri = path_to_url(path).unwrap();
        assert_eq!(uri.scheme(), "untitled");
        assert_eq!(uri.path(), "test");

        let path = url_to_path(&uri);
        assert_eq!(path, Path::new("/untitled/test").clean());
        assert!(is_untitled_path(&path));
    }

    #[test]
    fn unnamed_buffer() {
        // https://github.com/neovim/nvim-lspconfig/pull/2226
        let uri = EMPTY_URL.clone();
        let path = url_to_path(&uri);
        assert_eq!(path, Path::new("/untitled/nEoViM-BuG"));

        let uri2 = path_to_url(&path).unwrap();
        assert_eq!(EMPTY_URL.clone(), uri2);
    }
    
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_path_to_url_file_scheme_roundtrip() {
        // This test uses a Unix-style path; skip on Windows where
        // `Url::to_file_path` semantics differ for such inputs.
        #[cfg(unix)]
        {
            let p = PathBuf::from("/tmp/example file.typ");
            let url = path_to_url(&p).expect("file path to url");
            assert_eq!(url.scheme(), "file");
            // spaces should be percent-encoded
            assert!(url.as_str().contains("%20"));

            // url_to_file_path should give us back a cleaned absolute path
            let back = url_to_file_path(&url);
            assert!(back.is_absolute());
            assert!(back.ends_with("example file.typ"));
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), windows))]
    #[test]
    fn test_path_to_url_file_scheme_roundtrip_windows() {
        let p = PathBuf::from("C:\\Temp\\example file.typ");
        let url = path_to_url(&p).expect("file path to url");
        assert_eq!(url.scheme(), "file");
        // spaces should be percent-encoded
        assert!(url.as_str().contains("%20"));

        let back = url_to_file_path(&url);
        assert!(back.is_absolute());
        assert!(back.ends_with("example file.typ"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_path_to_url_untitled_special_case() {
        let p = PathBuf::from("/untitled/nEoViM-BuG");
        let url = path_to_url(&p).expect("untitled url");
        // Special case maps to EMPTY_URL which is a placeholder file:// URI
        assert_eq!(url.scheme(), "file");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_url_to_path_virtual_scheme() {
        let url = Url::parse("oct:/workspace/My File.typ").unwrap();
        let p = url_to_path(&url);
        // scheme should be embedded back into the path text, with decoded spaces
        assert_eq!(p.to_string_lossy(), "oct:/workspace/My File.typ");
    }
}
