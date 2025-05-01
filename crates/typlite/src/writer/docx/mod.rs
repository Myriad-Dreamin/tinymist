//! DOCX converter implementation using docx-rs
//!
//! This module is organized into several main components:
//! - Writer: Functionality for rendering intermediate DocxNode structure to DOCX format
//! - Styles: Document style management
//! - Numbering: List numbering management
//! - Node structures: DocxNode and DocxInline representing document structure

mod image_processor;
mod numbering;
mod styles;
mod writer;

pub use writer::DocxWriter;
