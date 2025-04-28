//! Common types and interfaces for the conversion system

use cmark_writer::ast::{CustomNodeWriter, Node};
use cmark_writer::derive_custom_node;
use cmark_writer::WriteResult;
use ecow::EcoString;
use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState {
    Ordered,
    Unordered,
}

/// Valid formats for the conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Md,
    LaTeX,
    Docx,
}

/// Figure node implementation for all formats
#[derive(Debug, PartialEq, Clone)]
pub struct FigureNode {
    /// The main content of the figure, can be any block node
    pub body: Box<Node>,
    /// The caption text for the figure
    pub caption: String,
}

derive_custom_node!(FigureNode);
impl FigureNode {
    fn write_custom(&self, writer: &mut dyn CustomNodeWriter) -> WriteResult<()> {
        let mut temp_writer = cmark_writer::writer::CommonMarkWriter::new();
        temp_writer.write(&self.body)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        writer.write_str("\n")?;
        writer.write_str(&self.caption)?;
        Ok(())
    }

    fn is_block_custom(&self) -> bool {
        true
    }
}

/// External Frame node for handling frames stored as external files
#[derive(Debug, PartialEq, Clone)]
pub struct ExternalFrameNode {
    /// The path to the external file containing the frame
    pub file_path: PathBuf,
    /// Alternative text for the frame
    pub alt_text: String,
    /// Original SVG data (needed for DOCX that still embeds images)
    pub svg_data: String,
}

derive_custom_node!(ExternalFrameNode);
impl ExternalFrameNode {
    fn write_custom(&self, writer: &mut dyn CustomNodeWriter) -> WriteResult<()> {
        // The actual handling is implemented in format-specific writers
        writer.write_str(&format!("![{}]({})", self.alt_text, self.file_path.display()))?;
        Ok(())
    }

    fn is_block_custom(&self) -> bool {
        true
    }
}

/// Common writer interface for different formats
pub trait FormatWriter {
    /// Write AST document to output format
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()>;

    /// Write AST document to vector
    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>>;
}
