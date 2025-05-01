//! Writer implementations for different output formats

pub mod docx;
pub mod latex;
pub mod markdown;

pub use self::docx::DocxWriter;
pub use latex::LaTeXWriter;
pub use markdown::MarkdownWriter;

use crate::common::{Format, FormatWriter};

/// Create a writer instance based on the specified format
pub fn create_writer(format: Format) -> Box<dyn FormatWriter> {
    match format {
        Format::Md => Box::new(markdown::MarkdownWriter::new()),
        Format::LaTeX => Box::new(latex::LaTeXWriter::new()),
        Format::Docx => Box::new(docx::DocxWriter::new()),
    }
}

pub struct WriterFactory;

impl WriterFactory {
    pub fn create(format: Format) -> Box<dyn FormatWriter> {
        create_writer(format)
    }
}
