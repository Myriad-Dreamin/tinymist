//! Text export utilities.

use core::fmt;
use tinymist_std::typst::TypstDocument;

/// A full text digest of a document.
pub struct FullTextDigest(pub TypstDocument);

impl FullTextDigest {
    fn export_frame(f: &mut fmt::Formatter<'_>, doc: &typst::layout::Frame) -> fmt::Result {
        for (_, item) in doc.items() {
            Self::export_item(f, item)?;
        }
        #[cfg(not(feature = "no-content-hint"))]
        {
            use std::fmt::Write;
            let c = doc.content_hint();
            if c != '\0' {
                f.write_char(c)?;
            }
        }

        Ok(())
    }

    fn export_item(f: &mut fmt::Formatter<'_>, item: &typst::layout::FrameItem) -> fmt::Result {
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => Self::export_frame(f, &g.frame),
            Text(t) => f.write_str(t.text.as_str()),
            Link(..) | Tag(..) | Shape(..) | Image(..) => Ok(()),
        }
    }
}

impl fmt::Display for FullTextDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TypstDocument::Paged(paged_doc) => {
                for page in paged_doc.pages.iter() {
                    Self::export_frame(f, &page.frame)?;
                }
                Ok(())
            }
        }
    }
}
