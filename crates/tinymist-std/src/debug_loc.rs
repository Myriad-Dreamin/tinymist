//! The debug location that can be used to locate a position in a document or a
//! file.

use core::fmt;

use serde::{Deserialize, Serialize};

/// A serializable physical position in a document.
///
/// Note that it uses [`f32`] instead of [`f64`] as same as
/// `TypstPosition` for the coordinates to improve both performance
/// of serialization and calculation. It does sacrifice the floating
/// precision, but it is enough in our use cases.
///
/// Also see `TypstPosition`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentPosition {
    /// The page, starting at 1.
    pub page_no: usize,
    /// The exact x-coordinate on the page (from the left, as usual).
    pub x: f32,
    /// The exact y-coordinate on the page (from the top, as usual).
    pub y: f32,
}

// impl From<TypstPosition> for DocumentPosition {
//     fn from(position: TypstPosition) -> Self {
//         Self {
//             page_no: position.page.into(),
//             x: position.point.x.to_pt() as f32,
//             y: position.point.y.to_pt() as f32,
//         }
//     }
// }

/// Raw representation of a source span.
pub type RawSourceSpan = u64;

/// A resolved source (text) location.
///
/// See [`CharPosition`] for the definition of the position inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLocation {
    /// The file path.
    pub filepath: String,
}

/// A char position represented in form of line and column.
/// The position is encoded in Utf-8 or Utf-16, and the encoding is
/// determined by usage.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
pub struct CharPosition {
    /// The line number, starting at 0.
    pub line: usize,
    /// The column number, starting at 0.
    pub column: usize,
}

impl fmt::Display for CharPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

impl From<Option<(usize, usize)>> for CharPosition {
    fn from(loc: Option<(usize, usize)>) -> Self {
        let (start, end) = loc.unwrap_or_default();
        CharPosition {
            line: start,
            column: end,
        }
    }
}

/// A resolved source (text) location.
///
/// See [`CharPosition`] for the definition of the position inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    /// The file path.
    pub filepath: String,
    /// The position in the file.
    pub pos: CharPosition,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn from_flat(
        flat: FlatSourceLocation,
        i: &impl std::ops::Index<usize, Output = FileLocation>,
    ) -> Self {
        Self {
            filepath: i[flat.filepath as usize].filepath.clone(),
            pos: flat.pos,
        }
    }
}

/// A flat resolved source (text) location.
///
/// See [`CharPosition`] for the definition of the position inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatSourceLocation {
    /// The file path.
    pub filepath: u32,
    /// The position in the file.
    pub pos: CharPosition,
}

/// A resolved file range.
///
/// See [`CharPosition`] for the definition of the position inside a file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharRange {
    /// The start position.
    pub start: CharPosition,
    /// The end position.
    pub end: CharPosition,
}

impl fmt::Display for CharRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}-{}", self.start, self.end)
        }
    }
}

/// A resolved source (text) range.
///
/// See [`CharPosition`] for the definition of the position inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRange {
    /// The file path.
    pub path: String,
    /// The range in the file.
    pub range: CharRange,
}

#[cfg(feature = "typst")]
mod typst_ext {
    pub use typst::layout::Position as TypstPosition;

    /// Unevaluated source span.
    /// The raw source span is unsafe to serialize and deserialize.
    /// Because the real source location is only known during liveness of
    /// the compiled document.
    pub type SourceSpan = typst::syntax::Span;

    /// Unevaluated source span with offset.
    ///
    /// It adds an additional offset relative to the start of the span.
    ///
    /// The offset is usually generated when the location is inside of some
    /// text or string content.
    #[derive(Debug, Clone, Copy)]
    pub struct SourceSpanOffset {
        /// The source span.
        pub span: SourceSpan,
        /// The offset relative to the start of the span. This is usually useful
        /// if the location is not a span created by the parser.
        pub offset: usize,
    }

    /// Lifts a [`SourceSpan`] to [`SourceSpanOffset`].
    impl From<SourceSpan> for SourceSpanOffset {
        fn from(span: SourceSpan) -> Self {
            Self { span, offset: 0 }
        }
    }

    /// Converts a [`SourceSpan`] and an in-text offset to [`SourceSpanOffset`].
    impl From<(SourceSpan, u16)> for SourceSpanOffset {
        fn from((span, offset): (SourceSpan, u16)) -> Self {
            Self {
                span,
                offset: offset as usize,
            }
        }
    }
}
#[cfg(feature = "typst")]
pub use typst_ext::*;

/// A point on the element tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementPoint {
    /// The element kind.
    pub kind: u32,
    /// The index of the element.
    pub index: u32,
    /// The fingerprint of the element.
    pub fingerprint: String,
}

impl From<(u32, u32, String)> for ElementPoint {
    fn from((kind, index, fingerprint): (u32, u32, String)) -> Self {
        Self {
            kind,
            index,
            fingerprint,
        }
    }
}

/// A file system data source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FsDataSource {
    /// The name of the data source.
    pub path: String,
}

/// A in-memory data source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct MemoryDataSource {
    /// The name of the data source.
    pub name: String,
}

/// Data source for a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind")]
pub enum DataSource {
    /// File system data source.
    #[serde(rename = "fs")]
    Fs(FsDataSource),
    /// Memory data source.
    #[serde(rename = "memory")]
    Memory(MemoryDataSource),
}
