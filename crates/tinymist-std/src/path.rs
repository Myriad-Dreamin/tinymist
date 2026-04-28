//! Path utilities.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use lsp_types::Url;
pub use path_clean::PathClean;

/// Gets the path cleaned as a unix-style string.
pub fn unix_slash(root: &Path) -> String {
    let mut res = String::with_capacity(root.as_os_str().len());
    let mut parent_norm = false;
    for comp in root.components() {
        match comp {
            Component::Prefix(p) => {
                res.push_str(&p.as_os_str().to_string_lossy());
                parent_norm = false;
            }
            Component::RootDir => {
                res.push('/');
                parent_norm = false;
            }
            Component::CurDir => {
                parent_norm = false;
            }
            Component::ParentDir => {
                if parent_norm {
                    res.push('/');
                }
                res.push_str("..");
                parent_norm = true;
            }
            Component::Normal(p) => {
                if parent_norm {
                    res.push('/');
                }
                res.push_str(&p.to_string_lossy());
                parent_norm = true;
            }
        }
    }

    if res.is_empty() {
        res.push('.');
    }

    res
}

/// Parses if a string has legal schema prefix, and is parsed as a `Url`.
/// Currently supported schemes are: `untitled`, `file`, `oct`.
/// Note that some schemes like `zhihu` on unix is rejected and viewed as a
/// normal path.
pub fn parse_uri(s: &str) -> Option<Url> {
    // todo: more schemes to support
    if s.starts_with("untitled:") || s.starts_with("file:") || s.starts_with("oct:") {
        // `url_to_path` encodes the uri scheme into the path while preserving forward
        // slashes so that a delegated filesystem can turn it back into a proper URI
        return Url::parse(s).ok();
    }
    None
}

/// Gets the path cleaned as a platform-style string.
pub use path_clean::clean;

/// Construct a relative path from a provided base directory path to the
/// provided path.
pub fn diff(path: &Path, base: &Path) -> Option<PathBuf> {
    // Because of <https://github.com/Manishearth/pathdiff/issues/8>, we have to clean the path
    // before diff.
    fn clean_for_diff(p: &Path) -> Cow<'_, Path> {
        if p.components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            Cow::Owned(p.clean())
        } else {
            Cow::Borrowed(p)
        }
    }

    pathdiff::diff_paths(clean_for_diff(path).as_ref(), clean_for_diff(base).as_ref())
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use super::{PathClean, clean as inner_path_clean, parse_uri, unix_slash};

    pub fn clean<P: AsRef<Path>>(path: P) -> String {
        unix_slash(&inner_path_clean(path))
    }

    /// Checks if a string looks like a URI.
    fn looks_like_uri(s: &str) -> bool {
        parse_uri(s).is_some()
    }

    #[test]
    fn test_unix_slash() {
        if cfg!(target_os = "windows") {
            // windows group
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\b\\c")),
                "C:/Users/a/b/c"
            );
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\b\\c\\")),
                "C:/Users/a/b/c"
            );
            assert_eq!(unix_slash(std::path::Path::new("a\\b\\c")), "a/b/c");
            assert_eq!(unix_slash(std::path::Path::new("C:\\")), "C:/");
            assert_eq!(unix_slash(std::path::Path::new("C:\\\\")), "C:/");
            assert_eq!(unix_slash(std::path::Path::new("C:")), "C:");
            assert_eq!(unix_slash(std::path::Path::new("C:\\a")), "C:/a");
            assert_eq!(unix_slash(std::path::Path::new("C:\\a\\")), "C:/a");
            assert_eq!(unix_slash(std::path::Path::new("C:\\a\\b")), "C:/a/b");
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\..\\b\\c")),
                "C:/Users/a/../b/c"
            );
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\..\\b\\c\\")),
                "C:/Users/a/../b/c"
            );
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\..\\..")),
                "C:/Users/a/../.."
            );
            assert_eq!(
                unix_slash(std::path::Path::new("C:\\Users\\a\\..\\..\\")),
                "C:/Users/a/../.."
            );
        }
        // unix group
        assert_eq!(unix_slash(std::path::Path::new("/a/b/c")), "/a/b/c");
        assert_eq!(unix_slash(std::path::Path::new("/a/b/c/")), "/a/b/c");
        assert_eq!(unix_slash(std::path::Path::new("/")), "/");
        assert_eq!(unix_slash(std::path::Path::new("//")), "/");
        assert_eq!(unix_slash(std::path::Path::new("a")), "a");
        assert_eq!(unix_slash(std::path::Path::new("a/")), "a");
        assert_eq!(unix_slash(std::path::Path::new("a/b")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new("a/b/")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new("a/..")), "a/..");
        assert_eq!(unix_slash(std::path::Path::new("a/../")), "a/..");
        assert_eq!(unix_slash(std::path::Path::new("a/../..")), "a/../..");
        assert_eq!(unix_slash(std::path::Path::new("a/../../")), "a/../..");
        assert_eq!(unix_slash(std::path::Path::new("a/./b")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new("a/./b/")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new(".")), ".");
        assert_eq!(unix_slash(std::path::Path::new("./")), ".");
        assert_eq!(unix_slash(std::path::Path::new("./a")), "a");
        assert_eq!(unix_slash(std::path::Path::new("./a/")), "a");
        assert_eq!(unix_slash(std::path::Path::new("./a/b")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new("./a/b/")), "a/b");
        assert_eq!(unix_slash(std::path::Path::new("./a/./b/")), "a/b");
    }

    #[test]
    fn test_path_clean_empty_path_is_current_dir() {
        assert_eq!(clean(""), ".");
    }

    #[test]
    fn test_path_clean_clean_paths_dont_change() {
        let tests = vec![(".", "."), ("..", ".."), ("/", "/")];

        for test in tests {
            assert_eq!(clean(test.0), test.1);
        }
    }

    #[test]
    fn test_path_clean_replace_multiple_slashes() {
        let tests = vec![
            ("/", "/"),
            ("//", "/"),
            ("///", "/"),
            (".//", "."),
            ("//..", "/"),
            ("..//", ".."),
            ("/..//", "/"),
            ("/.//./", "/"),
            ("././/./", "."),
            ("path//to///thing", "path/to/thing"),
            ("/path//to///thing", "/path/to/thing"),
        ];

        for test in tests {
            assert_eq!(clean(test.0), test.1);
        }
    }

    #[test]
    fn test_path_clean_eliminate_current_dir() {
        let tests = vec![
            ("./", "."),
            ("/./", "/"),
            ("./test", "test"),
            ("./test/./path", "test/path"),
            ("/test/./path/", "/test/path"),
            ("test/path/.", "test/path"),
        ];

        for test in tests {
            assert_eq!(clean(test.0), test.1);
        }
    }

    #[test]
    fn test_path_clean_eliminate_parent_dir() {
        let tests = vec![
            ("/..", "/"),
            ("/../test", "/test"),
            ("test/..", "."),
            ("test/path/..", "test"),
            ("test/../path", "path"),
            ("/test/../path", "/path"),
            ("test/path/../../", "."),
            ("test/path/../../..", ".."),
            ("/test/path/../../..", "/"),
            ("/test/path/../../../..", "/"),
            ("test/path/../../../..", "../.."),
            ("test/path/../../another/path", "another/path"),
            ("test/path/../../another/path/..", "another"),
            ("../test", "../test"),
            ("../test/", "../test"),
            ("../test/path", "../test/path"),
            ("../test/..", ".."),
        ];

        for test in tests {
            assert_eq!(clean(test.0), test.1);
        }
    }

    #[test]
    fn test_path_clean_pathbuf_trait() {
        assert_eq!(
            unix_slash(&PathBuf::from("/test/../path/").clean()),
            "/path"
        );
    }

    #[test]
    fn test_path_clean_path_trait() {
        assert_eq!(unix_slash(&Path::new("/test/../path/").clean()), "/path");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_path_clean_windows_paths() {
        let tests = vec![
            ("\\..", "/"),
            ("\\..\\test", "/test"),
            ("test\\..", "."),
            ("test\\path\\..\\..\\..", ".."),
            ("test\\path/..\\../another\\path", "another/path"), // Mixed
            ("test\\path\\my/path", "test/path/my/path"),        // Mixed 2
            ("/dir\\../otherDir/test.json", "/otherDir/test.json"), // User example
            ("c:\\test\\..", "c:/"),                             // issue #12
            ("c:/test/..", "c:/"),                               // issue #12
        ];

        for test in tests {
            assert_eq!(clean(test.0), test.1);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_looks_like_uri_valid_schemes() {
        // These schemes are recognized by tinymist.
        assert!(looks_like_uri("file:/C:/Windows"));
        assert!(looks_like_uri("oct:/workspace/file typst"));

        // while these URI are valid,
        // they are not identified as URI and is valid path on unix.
        assert!(!looks_like_uri("http:/path"));
        assert!(!looks_like_uri("custom-scheme:/abc"));
        assert!(!looks_like_uri("a+1.2-3:/zzz"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_looks_like_uri_rejects_drive_letters_and_edge_cases() {
        // Single-letter "scheme" like windows drive should be rejected
        assert!(!looks_like_uri("C:/Windows"));
        assert!(!looks_like_uri("D:"));

        // No colon -> not a URI
        assert!(!looks_like_uri("/usr/bin"));
        assert!(!looks_like_uri("relative/path"));

        // Invalid first character
        assert!(!looks_like_uri("1abc:/path"));
        assert!(!looks_like_uri("+abc:/path"));

        // Invalid characters in scheme
        assert!(!looks_like_uri("ab*c:/path"));
        assert!(!looks_like_uri("ab c:/path"));
    }
}
