//! Table support for CommonMark and GFM
//!
//! This module provides utilities for working with tables in CommonMark and GitHub Flavored Markdown.
//! When the `gfm` feature is enabled, additional alignment functionality is available.

use super::{Node, TableCell, TableCellKind, TableRow, TableRowKind};

/// Table builder for creating tables with customized content
///
/// This builder provides a fluent API for creating tables with
/// headers and rows.
#[derive(Debug, Clone)]
pub struct TableBuilder {
    /// Table header cells
    headers: Vec<Node>,
    /// Table rows, each containing multiple cells
    rows: Vec<Vec<Node>>,
    /// Explicit column count
    columns: Option<usize>,
    /// Column alignments (left, center, right, or none)
    alignments: Vec<super::TableAlignment>,
}

fn convert_rows(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Vec<TableRow> {
    let mut result = Vec::new();

    if !headers.is_empty() {
        let cells = headers
            .into_iter()
            .map(|node| TableCell::new(TableCellKind::Header, node))
            .collect();
        result.push(TableRow {
            kind: TableRowKind::Head,
            cells,
        });
    }

    for row in rows {
        let cells = row
            .into_iter()
            .map(|node| TableCell::new(TableCellKind::Data, node))
            .collect();
        result.push(TableRow {
            kind: TableRowKind::Body,
            cells,
        });
    }

    result
}

impl TableBuilder {
    /// Creates a new table builder
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            columns: None,
            alignments: Vec::new(),
        }
    }

    /// Sets the table headers
    ///
    /// # Arguments
    /// * `headers` - Vector of nodes representing header cells
    pub fn headers(mut self, headers: Vec<Node>) -> Self {
        self.headers = headers;
        self
    }

    /// Adds a single row to the table
    ///
    /// # Arguments
    /// * `row` - Vector of nodes representing cells in a row
    pub fn add_row(mut self, row: Vec<Node>) -> Self {
        self.rows.push(row);
        self
    }

    /// Adds multiple rows to the table
    ///
    /// # Arguments
    /// * `rows` - Vector of rows, each containing cell nodes
    pub fn add_rows(mut self, rows: Vec<Vec<Node>>) -> Self {
        self.rows.extend(rows);
        self
    }

    /// Sets the total number of columns for the table.
    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = Some(columns.max(1));
        self
    }

    /// Builds the final table node with alignment information
    ///
    /// # Returns
    /// A Node::Table with the specified headers, alignments, and rows
    pub fn build(self) -> Node {
        let TableBuilder {
            headers,
            rows,
            columns,
            alignments,
        } = self;

        let rows = convert_rows(headers, rows);
        let columns = columns
            .unwrap_or_else(|| rows.first().map(|row| row.cells.len()).unwrap_or(0))
            .max(1);
        let mut alignments = if alignments.is_empty() {
            vec![super::TableAlignment::None; columns]
        } else {
            alignments
        };
        if alignments.len() < columns {
            alignments.resize(columns, super::TableAlignment::None);
        } else if alignments.len() > columns {
            alignments.truncate(columns);
        }
        Node::Table {
            columns,
            rows,
            alignments,
        }
    }

    /// Sets all columns to use the same alignment (only available with `gfm` feature)
    ///
    /// # Arguments
    /// * `alignment` - Alignment to apply to all columns
    pub fn align_all(mut self, alignment: super::TableAlignment) -> Self {
        self.alignments = vec![alignment; self.headers.len().max(1)];
        self
    }

    /// Sets the alignment for a specific column (only available with `gfm` feature)
    ///
    /// # Arguments
    /// * `column` - Zero-based column index
    /// * `alignment` - Alignment to apply to the column
    pub fn align_column(mut self, column: usize, alignment: super::TableAlignment) -> Self {
        if column >= self.alignments.len() {
            self.alignments
                .resize(column + 1, super::TableAlignment::default());
        }
        self.alignments[column] = alignment;
        self
    }

    /// Sets alignments for multiple columns (only available with `gfm` feature)
    ///
    /// # Arguments
    /// * `alignments` - Vector of alignments, one for each column
    pub fn alignments(mut self, alignments: Vec<super::TableAlignment>) -> Self {
        self.alignments = alignments;
        self
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates a simple table
///
/// # Arguments
/// * `headers` - Vector of nodes representing header cells
/// * `rows` - Vector of rows, each containing cell nodes
///
/// # Returns
/// A table node with the provided headers and rows
pub fn simple_table(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Node {
    TableBuilder::new().headers(headers).add_rows(rows).build()
}

/// Creates a centered table with all columns center-aligned (only available with `gfm` feature)
///
/// # Arguments
/// * `headers` - Vector of nodes representing header cells
/// * `rows` - Vector of rows, each containing cell nodes
///
/// # Returns
/// A table node with all columns center-aligned
#[cfg(feature = "gfm")]
pub fn centered_table(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Node {
    TableBuilder::new()
        .headers(headers)
        .align_all(super::TableAlignment::Center)
        .add_rows(rows)
        .build()
}
