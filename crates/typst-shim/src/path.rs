//! Compatibility helpers for Typst path resolution.

use typst::diag::HintedStrResult;
use typst::foundations::PathOrStr;
use typst::syntax::{FileId, RootedPath};

/// Resolves a Typst path string relative to a file id.
///
/// This delegates to Typst's own [`PathOrStr::resolve`] implementation so
/// callers keep the same relative-path, absolute-path, normalization, and
/// escaping semantics as Typst evaluation.
pub fn resolve_path_from_id(within: FileId, path: &str) -> HintedStrResult<RootedPath> {
    PathOrStr::Str(path.into()).resolve(within)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use typst::syntax::package::PackageSpec;
    use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};

    use super::resolve_path_from_id;

    fn file_id(root: VirtualRoot, path: &str) -> FileId {
        RootedPath::new(root, VirtualPath::new(path).expect("valid virtual path")).intern()
    }

    fn package(spec: &str) -> PackageSpec {
        PackageSpec::from_str(spec).expect("valid package spec")
    }

    #[test]
    fn resolve_path_from_id_uses_parent_directory_for_relative_paths() {
        let current = file_id(VirtualRoot::Project, "/chapter/main.typ");
        let resolved = resolve_path_from_id(current, "assets/logo.svg").unwrap();

        assert_eq!(resolved.root(), current.root());
        assert_eq!(
            resolved.vpath().get_with_slash(),
            "/chapter/assets/logo.svg"
        );
    }

    #[test]
    fn resolve_path_from_id_keeps_root_fallback_when_base_has_no_parent() {
        let current = file_id(VirtualRoot::Project, "/");
        let resolved = resolve_path_from_id(current, "main.typ").unwrap();

        assert_eq!(resolved.root(), current.root());
        assert_eq!(resolved.vpath().get_with_slash(), "/main.typ");
    }

    #[test]
    fn resolve_path_from_id_resolves_absolute_paths_within_same_root() {
        let root = VirtualRoot::Package(package("@preview/example:0.1.0"));
        let current = file_id(root, "/chapter/main.typ");
        let resolved = resolve_path_from_id(current, "/assets/logo.svg").unwrap();

        assert_eq!(resolved.root(), current.root());
        assert_eq!(resolved.vpath().get_with_slash(), "/assets/logo.svg");
    }

    #[test]
    fn resolve_path_from_id_rejects_paths_that_escape_the_root() {
        let current = file_id(VirtualRoot::Project, "/main.typ");

        assert!(resolve_path_from_id(current, "../outside.typ").is_err());
    }
}
