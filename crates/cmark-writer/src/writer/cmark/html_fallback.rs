use crate::ast::{Node, TableAlignment, TableRow};
use crate::error::WriteResult;
use crate::writer::html::HtmlWriter;

use super::CommonMarkWriter;

impl CommonMarkWriter {
    /// Write a table using the HTML backend (fallback for complex cells).
    pub(crate) fn write_table_as_html(
        &mut self,
        columns: usize,
        rows: &[TableRow],
        alignments: &[TableAlignment],
    ) -> WriteResult<()> {
        let mut html_writer = HtmlWriter::new();
        let table_node = Node::Table {
            columns,
            rows: rows.to_vec(),
            alignments: alignments.to_vec(),
        };

        html_writer.write_node(&table_node).map_err(|e| {
            crate::error::WriteError::HtmlFallbackError(
                format!("Failed to write GFM table as HTML: {e}").into(),
            )
        })?;

        let html_output = html_writer.into_string().map_err(|e| {
            crate::error::WriteError::HtmlFallbackError(
                format!("Failed to finalize GFM table HTML output: {e}").into(),
            )
        })?;
        self.buffer.push_str(&html_output);
        Ok(())
    }
}
