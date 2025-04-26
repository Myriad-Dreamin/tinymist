//! DOCX converter implementation using docx-rs
//!
//! This module is organized into several main components:
//! - Converter: Core functionality for converting HTML to intermediate DocxNode structure
//! - Writer: Functionality for rendering intermediate DocxNode structure to DOCX format
//! - Styles: Document style management 
//! - Numbering: List numbering management
//! - Node structures: DocxNode and DocxInline representing document structure

mod types;
mod utils;
mod writer;
mod converter;
mod styles;
mod numbering;

pub use converter::DocxConverter;