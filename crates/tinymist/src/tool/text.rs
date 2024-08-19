//! Text export utilities.

use core::fmt;
use std::sync::Arc;

use typst_ts_core::TypstDocument;

/// A full text digest of a document.
pub struct FullTextDigest(pub Arc<TypstDocument>);

impl FullTextDigest {
    fn export_frame(f: &mut fmt::Formatter<'_>, doc: &typst::layout::Frame) -> fmt::Result {
        for (_, item) in doc.items() {
            Self::export_item(f, item)?;
        }

        Ok(())
    }

    fn export_item(f: &mut fmt::Formatter<'_>, item: &typst::layout::FrameItem) -> fmt::Result {
        use typst::introspection::Meta::*;
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => Self::export_frame(f, &g.frame),
            Text(t) => f.write_str(t.text.as_str()),
            #[cfg(not(feature = "no-content-hint"))]
            Meta(ContentHint(c), _) => f.write_char(*c),
            Meta(Link(..) | Elem(..) | Hide, _) | Shape(..) | Image(..) => Ok(()),
        }
    }
}

impl fmt::Display for FullTextDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for page in self.0.pages.iter() {
            Self::export_frame(f, &page.frame)?;
        }
        Ok(())
    }
}
