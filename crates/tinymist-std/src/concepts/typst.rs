//! Re-export of the typst crate.
pub(crate) mod well_known {
    pub use typst::foundations::Bytes;

    pub use typst::utils::LazyHash;

    /// Although this is not good to expose this, we make an alias here to
    /// let it as a part of typst-ts.
    pub use typst::syntax::FileId as TypstFileId;

    pub use typst::World as TypstWorld;

    pub use typst::layout::Abs as TypstAbs;

    pub use typst::layout::PagedDocument as TypstPagedDocument;

    pub use typst::html::HtmlDocument as TypstHtmlDocument;

    pub use typst::text::Font as TypstFont;

    pub use typst::foundations::Dict as TypstDict;

    pub use typst::foundations::Datetime as TypstDatetime;

    pub use typst::{diag, foundations, syntax};
}

/// The enum of all well-known Typst documents.
#[derive(Debug, Clone)]
pub enum TypstDocument {
    /// The document compiled with `paged` target.
    Paged(Arc<well_known::TypstPagedDocument>),
    /// The document compiled with `html` target.
    Html(Arc<well_known::TypstHtmlDocument>),
}

impl TypstDocument {
    /// Gets the number of pages in the document.
    pub fn num_of_pages(&self) -> u32 {
        match self {
            Self::Paged(doc) => doc.pages.len() as u32,
            Self::Html(_doc) => 1u32,
        }
    }

    /// Gets details about the document.
    pub fn info(&self) -> &typst::model::DocumentInfo {
        match self {
            Self::Paged(doc) => &doc.info,
            Self::Html(doc) => &doc.info,
        }
    }

    /// Gets the introspector that can be queried for elements and their
    /// positions.
    pub fn introspector(&self) -> &typst::introspection::Introspector {
        match self {
            Self::Paged(doc) => &doc.introspector,
            Self::Html(doc) => &doc.introspector,
        }
    }
}

impl From<Arc<well_known::TypstPagedDocument>> for TypstDocument {
    fn from(doc: Arc<well_known::TypstPagedDocument>) -> Self {
        Self::Paged(doc)
    }
}

impl From<Arc<well_known::TypstHtmlDocument>> for TypstDocument {
    fn from(doc: Arc<well_known::TypstHtmlDocument>) -> Self {
        Self::Html(doc)
    }
}

use std::sync::Arc;

pub use well_known::*;

/// The prelude of the Typst module.
pub mod prelude {
    pub use comemo::Prehashed;
    pub use ecow::{eco_format, eco_vec, EcoString, EcoVec};
}
