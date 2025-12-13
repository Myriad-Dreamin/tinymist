//! Markdown writer implementation.

mod emit;
mod escape;
mod render_html;

mod lists;
mod tables;

use ecow::EcoString;

use crate::Result;
use crate::ir;
use crate::writer::IrFormatWriter;

/// Markdown writer implementation.
#[derive(Default)]
pub struct MarkdownWriter {}

impl MarkdownWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl IrFormatWriter for MarkdownWriter {
    fn write_ir_eco(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        let mut emitter = emit::IrMarkdownEmitter::new();
        emitter.write_document(document, output)
    }

    fn write_ir_vec(&mut self, _document: &ir::Document) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}
