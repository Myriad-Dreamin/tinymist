use crate::ast::Node;
#[cfg(feature = "gfm")]
use crate::ast::TableAlignment;
use crate::error::WriteResult;
use crate::writer::html::HtmlWriter;

use super::CommonMarkWriter;

impl CommonMarkWriter {
    /// Write a table using the HTML backend (fallback for complex cells).
    pub(crate) fn write_table_as_html(
        &mut self,
        headers: &[Node],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        let mut html_writer = HtmlWriter::new();
        let table_node = Node::Table {
            headers: headers.to_vec(),
            #[cfg(feature = "gfm")]
            alignments: vec![],
            rows: rows.to_vec(),
        };

        html_writer.write_node(&table_node).map_err(|e| {
            crate::error::WriteError::HtmlFallbackError(
                format!("Failed to write table as HTML: {e}").into(),
            )
        })?;

        let html_output = html_writer.into_string();
        self.buffer.push_str(&html_output);
        Ok(())
    }

    /// Write a GFM table with alignment using the HTML backend.
    #[cfg(feature = "gfm")]
    pub(crate) fn write_table_as_html_with_alignment(
        &mut self,
        headers: &[Node],
        alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        let mut html_writer = HtmlWriter::new();
        let table_node = Node::Table {
            headers: headers.to_vec(),
            alignments: alignments.to_vec(),
            rows: rows.to_vec(),
        };

        html_writer.write_node(&table_node).map_err(|e| {
            crate::error::WriteError::HtmlFallbackError(
                format!("Failed to write GFM table as HTML: {e}").into(),
            )
        })?;

        let html_output = html_writer.into_string();
        self.buffer.push_str(&html_output);
        Ok(())
    }
}
