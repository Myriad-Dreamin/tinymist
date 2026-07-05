//! Table support for CommonMark and GFM
//!
//! This module provides utilities for working with tables in CommonMark and GitHub Flavored Markdown.
//! When the `gfm` feature is enabled, additional alignment functionality is available.

use super::Node;

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
    #[cfg(feature = "gfm")]
    /// Column alignments (left, center, right, or none)
    alignments: Vec<super::TableAlignment>,
}

impl TableBuilder {
    /// Creates a new table builder
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            #[cfg(feature = "gfm")]
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

    /// Builds the final table node
    ///
    /// # Returns
    /// A Node::Table with the specified headers and rows
    #[cfg(not(feature = "gfm"))]
    pub fn build(self) -> Node {
        Node::Table {
            headers: self.headers,
            rows: self.rows,
        }
    }

    /// Builds the final table node with alignment information
    ///
    /// # Returns
    /// A Node::Table with the specified headers, alignments, and rows
    #[cfg(feature = "gfm")]
    pub fn build(self) -> Node {
        Node::Table {
            headers: self.headers,
            alignments: self.alignments,
            rows: self.rows,
        }
    }

    /// Sets all columns to use the same alignment (only available with `gfm` feature)
    ///
    /// # Arguments
    /// * `alignment` - Alignment to apply to all columns
    #[cfg(feature = "gfm")]
    pub fn align_all(mut self, alignment: super::TableAlignment) -> Self {
        self.alignments = vec![alignment; self.headers.len().max(1)];
        self
    }

    /// Sets the alignment for a specific column (only available with `gfm` feature)
    ///
    /// # Arguments
    /// * `column` - Zero-based column index
    /// * `alignment` - Alignment to apply to the column
    #[cfg(feature = "gfm")]
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
    #[cfg(feature = "gfm")]
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
