//! Writer implementations for different output formats

#[cfg(feature = "docx")]
pub mod docx;
pub mod latex;
pub mod markdown;
pub mod text;

#[cfg(feature = "docx")]
pub use self::docx::DocxWriter;
pub use latex::LaTeXWriter;
pub use markdown::MarkdownWriter;
pub use text::TextWriter;

use crate::common::{Format, FormatWriter};
use crate::ir;
use ecow::EcoString;
use crate::Result;

/// Writer interface for semantic IR documents.
pub trait IrFormatWriter {
    /// Write IR document to output format.
    fn write_ir_eco(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()>;

    /// Write IR document to vector.
    fn write_ir_vec(&mut self, document: &ir::Document) -> Result<Vec<u8>>;
}

/// Create a writer instance based on the specified format
pub fn create_writer(format: Format) -> Box<dyn FormatWriter> {
    match format {
        Format::Md => Box::new(markdown::MarkdownWriter::new()),
        Format::LaTeX => Box::new(latex::LaTeXWriter::new()),
        Format::Text => Box::new(text::TextWriter::new()),
        #[cfg(feature = "docx")]
        Format::Docx => Box::new(docx::DocxWriter::new()),
    }
}

/// Create an IR writer instance based on the specified format.
pub fn create_ir_writer(format: Format) -> Box<dyn IrFormatWriter> {
    match format {
        Format::Md => Box::new(markdown::MarkdownWriter::new()),
        Format::LaTeX => Box::new(latex::LaTeXWriter::new()),
        Format::Text => Box::new(text::TextWriter::new()),
        #[cfg(feature = "docx")]
        Format::Docx => Box::new(docx::DocxWriter::new()),
    }
}

pub struct WriterFactory;

impl WriterFactory {
    pub fn create(format: Format) -> Box<dyn FormatWriter> {
        create_writer(format)
    }

    pub fn create_ir(format: Format) -> Box<dyn IrFormatWriter> {
        create_ir_writer(format)
    }
}
