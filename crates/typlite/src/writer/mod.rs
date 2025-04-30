//! Writer implementations for different output formats

pub mod markdown;

pub use markdown::MarkdownWriter;

use crate::common::{Format, FormatWriter};

/// Create a writer instance based on the specified format
pub fn create_writer(format: Format) -> Box<dyn FormatWriter> {
    match format {
        Format::Md => Box::new(markdown::MarkdownWriter::new()),
        Format::LaTeX | Format::Docx => {
            panic!("LaTeX and Docx writers are not implemented yet")
        }
    }
}

pub struct WriterFactory;

impl WriterFactory {
    pub fn create(format: Format) -> Box<dyn FormatWriter> {
        create_writer(format)
    }
}
