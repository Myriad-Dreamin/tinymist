//! Markdown writer implementation

use cmark_writer::ast::Node;
use cmark_writer::writer::CommonMarkWriter;
use ecow::EcoString;

use crate::common::FormatWriter;
use crate::Result;

/// Markdown writer implementation
#[derive(Default)]
pub struct MarkdownWriter {}

impl MarkdownWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl FormatWriter for MarkdownWriter {
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()> {
        let mut writer = CommonMarkWriter::new();
        writer.write(document).expect("Failed to write document");
        output.push_str(&writer.into_string());
        Ok(())
    }

    fn write_vec(&mut self, _document: &Node) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}
