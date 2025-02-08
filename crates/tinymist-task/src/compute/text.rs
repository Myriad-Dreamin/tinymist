use core::fmt;
use std::sync::Arc;

use crate::ExportTextTask;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstDocument, TypstPagedDocument};
use tinymist_world::{CompilerFeat, ExportComputation, WorldComputeGraph};

pub struct TextExport;

impl TextExport {
    pub fn run_on_doc(doc: &TypstDocument) -> Result<String> {
        Ok(format!("{}", FullTextDigest(doc.clone())))
    }
}

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for TextExport {
    type Output = String;
    type Config = ExportTextTask;

    fn run(
        _g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        _config: &ExportTextTask,
    ) -> Result<String> {
        Self::run_on_doc(&TypstDocument::Paged(doc.clone()))
    }
}

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
