//! Path utilities.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

pub use path_clean::PathClean;

/// Get the path cleaned as a unix-style string.
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

/// Get the path cleaned as a platform-style string.
pub use path_clean::clean;

/// Construct a relative path from a provided base directory path to the
/// provided path.
pub fn diff(fr: &Path, to: &Path) -> Option<PathBuf> {
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

    pathdiff::diff_paths(clean_for_diff(fr).as_ref(), clean_for_diff(to).as_ref())
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use super::{clean as inner_path_clean, unix_slash, PathClean};

    pub fn clean<P: AsRef<Path>>(path: P) -> String {
        unix_slash(&inner_path_clean(path))
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
}
