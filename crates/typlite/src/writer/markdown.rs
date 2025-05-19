//! Markdown writer implementation

use cmark_writer::ast::Node;
use cmark_writer::writer::CommonMarkWriter;
use cmark_writer::WriterOptions;
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
        let mut writer = CommonMarkWriter::with_options(WriterOptions {
            strict: false,
            ..Default::default()
        });
        writer
            .write(document)
            .map_err(|e| format!("failed to write document: {}", e))?;
        output.push_str(&writer.into_string());
        Ok(())
    }

    fn write_vec(&mut self, _document: &Node) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}
