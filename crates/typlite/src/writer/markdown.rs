//! Markdown writer implementation

use cmark_writer::WriterOptions;
use cmark_writer::ast::Node;
use cmark_writer::writer::CommonMarkWriter;
use ecow::EcoString;

use crate::Result;
use crate::common::FormatWriter;
use crate::ir;
use crate::writer::IrFormatWriter;

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
            escape_special_chars: true,
            trim_paragraph_trailing_hard_breaks: true,
            ..Default::default()
        });
        writer
            .write(document)
            .map_err(|e| format!("failed to write document: {e}"))?;
        output.push_str(&writer.into_string());
        Ok(())
    }

    fn write_vec(&mut self, _document: &Node) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}

impl IrFormatWriter for MarkdownWriter {
    fn write_ir_eco(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        let ast = document.to_cmark();
        self.write_eco(&ast, output)
    }

    fn write_ir_vec(&mut self, _document: &ir::Document) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}
