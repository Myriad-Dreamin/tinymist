//! Text export utilities.

use core::fmt;
use reflexo_typst::TypstDocument;
use std::sync::Arc;

/// A full text digest of a document.
pub struct FullTextDigest(pub Arc<TypstDocument>);

impl FullTextDigest {
    fn export_frame(f: &mut fmt::Formatter<'_>, doc: &typst::layout::Frame) -> fmt::Result {
        #[cfg(not(feature = "no-content-hint"))]
        use std::fmt::Write;

        for (_, item) in doc.items() {
            Self::export_item(f, item)?;
        }

        #[cfg(not(feature = "no-content-hint"))]
        {
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
            Text(t) => {
                f.write_str(t.text.as_str())?;

                Ok(())
            }
            Link(..) | Tag(..) | Shape(..) | Image(..) => Ok(()),
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
