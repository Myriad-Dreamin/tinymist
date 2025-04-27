//! Core functionality for converting HTML to DOCX format

use typst::html::HtmlElement;

use crate::common::FormatWriter;
use crate::parser::{create_parser, Parser};
use crate::Result;
use crate::TypliteFeat;

use crate::writer::docx::writer::DocxWriter;

/// DOCX Converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
}

impl DocxConverter {
    /// Create a new DOCX converter
    pub fn new(feat: TypliteFeat) -> Self {
        Self { feat }
    }

    /// Convert HTML element to DOCX format
    pub fn convert(&mut self, root: &HtmlElement) -> Result<Vec<u8>> {
        // Parse HTML to AST using shared parser
        let parser = create_parser(self.feat.clone());
        let document = parser.parse(root)?; // Create and initialize DocxWriter
        let mut writer = DocxWriter::new();
        writer.write_vec(&document)
    }
}
