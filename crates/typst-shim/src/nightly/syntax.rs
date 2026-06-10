//! Typst Syntax
use std::ops::Range;
use std::path::Path;

use typst::syntax::DiagSpan;
use typst::syntax::DiagSpanKind;
use typst::syntax::LinkedNode;
use typst::syntax::RootedPath;
use typst::syntax::Side;
use typst::syntax::Source;
use typst::syntax::VirtualPath;
use typst::syntax::VirtualRoot;
use typst::syntax::package::PackageSpec;

/// Get the byte range for a diagnostic span within a source file.
pub fn source_range(source: &Source, span: impl Into<DiagSpan>) -> Option<Range<usize>> {
    match span.into().get() {
        DiagSpanKind::Detached => None,
        DiagSpanKind::Number { id, num, sub_range } if id == source.id() => {
            source.range(num, sub_range)
        }
        DiagSpanKind::Range { id, range } if id == source.id() => Some(range),
        _ => None,
    }
}

/// The `LinkedNodeExt` trait is designed for compatibility between new and old
/// versions of `typst`.
pub trait LinkedNodeExt: Sized {
    /// Get the leaf at the specified byte offset.
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self>;
}

impl LinkedNodeExt for LinkedNode<'_> {
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self> {
        self.leaf_at(cursor, Side::Before)
    }
}

/// The `VirtualPathExt` trait is designed for compatibility between new and old
/// versions of `typst`.
pub trait VirtualPathExt {
    /// Get the underlying path with a leading `/` or `\`.
    fn as_rooted_path_compat(&self) -> &Path;

    /// Get the underlying path without a leading `/` or `\`.
    fn as_rootless_path_compat(&self) -> &Path;
}

impl VirtualPathExt for VirtualPath {
    fn as_rooted_path_compat(&self) -> &Path {
        Path::new(self.get_with_slash())
    }

    fn as_rootless_path_compat(&self) -> &Path {
        Path::new(self.get_without_slash())
    }
}

/// The `RootedPathExt` trait is designed for compatibility between new and old
/// versions of `typst`.
pub trait RootedPathExt {
    /// The package the path resides in, if any.
    fn package_compat(&self) -> Option<&PackageSpec>;
}

impl RootedPathExt for RootedPath {
    fn package_compat(&self) -> Option<&PackageSpec> {
        match self.root() {
            VirtualRoot::Project => None,
            VirtualRoot::Package(package) => Some(package),
        }
    }
}
