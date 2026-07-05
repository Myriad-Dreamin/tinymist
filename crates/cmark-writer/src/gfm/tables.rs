//! GFM table support with alignment features
//!
//! This module provides utilities for working with GitHub Flavored Markdown tables,
//! including advanced alignment options.

use crate::ast::Node;
pub use crate::ast::{tables::TableBuilder, TableAlignment};

/// Re-export the core table building functionality from ast/tables
pub use crate::ast::tables::{centered_table, simple_table};

/// Creates a right-aligned table with all columns right-aligned
///
/// # Arguments
/// * `headers` - Vector of nodes representing header cells
/// * `rows` - Vector of rows, each containing cell nodes
///
/// # Returns
/// A table node with all columns right-aligned
pub fn right_aligned_table(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Node {
    TableBuilder::new()
        .headers(headers)
        .align_all(TableAlignment::Right)
        .add_rows(rows)
        .build()
}

/// Creates a table with alternating alignment (left, center, right, repeat)
///
/// # Arguments
/// * `headers` - Vector of nodes representing header cells
/// * `rows` - Vector of rows, each containing cell nodes
///
/// # Returns
/// A table node with alternating column alignment
pub fn alternating_table(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Node {
    let mut alignments = Vec::with_capacity(headers.len());

    // Create alternating alignment pattern
    for i in 0..headers.len() {
        let alignment = match i % 3 {
            0 => TableAlignment::Left,
            1 => TableAlignment::Center,
            _ => TableAlignment::Right,
        };
        alignments.push(alignment);
    }

    TableBuilder::new()
        .headers(headers)
        .alignments(alignments)
        .add_rows(rows)
        .build()
}
