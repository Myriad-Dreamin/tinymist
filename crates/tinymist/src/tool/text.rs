//! Text export utilities.

use core::fmt;
use reflexo_typst::TypstDocument;
use std::sync::Arc;

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
        #[cfg(not(feature = "no-content-hint"))]
        use std::fmt::Write;
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => Self::export_frame(f, &g.frame),
            Text(t) => f.write_str(t.text.as_str()),
            #[cfg(not(feature = "no-content-hint"))]
            ContentHint(c) => f.write_char(*c),
            Link(..) | Tag(..) | Shape(..) | Image(..) => Ok(()),
        }
    }
}

impl fmt::Display for FullTextDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_ref() {
            TypstDocument::Paged(paged_doc) => {
                for page in paged_doc.pages.iter() {
                    Self::export_frame(f, &page.frame)?;
                }
                Ok(())
            }
            _ => Err(fmt::Error),
        }
    }
}
