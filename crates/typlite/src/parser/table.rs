//! HTML table parsing module, processes the conversion of table elements

use cmark_writer::ast::Node;
use cmark_writer::gfm::TableAlignment;
use typst::html::{tag, HtmlElement, HtmlNode};

use crate::tags::md_tag;
use crate::Result;

use super::core::HtmlToAstParser;

/// Table parser
pub struct TableParser;

impl TableParser {
    /// Convert HTML table to CommonMark AST
    pub fn convert_table(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Option<Node>> {
        // Find the real table element
        let real_table_elem = Self::find_real_table_element(element);

        // Process the table (if found)
        if let Some(table) = real_table_elem {
            // Check if the table contains rowspan or colspan attributes
            // If it does, fall back to using HtmlElement
            if Self::table_has_complex_cells(table) {
                if let Ok(html_node) = parser.create_html_element(table) {
                    return Ok(Some(html_node));
                }
                return Ok(None);
            }

            let mut headers = Vec::new();
            let mut rows = Vec::new();
            let mut is_header = true;

            Self::extract_table_content(parser, table, &mut headers, &mut rows, &mut is_header)?;
            return Self::create_table_node(headers, rows);
        }

        Ok(None)
    }

    /// Find the real table element in the HTML structure
    fn find_real_table_element<'a>(element: &'a HtmlElement) -> Option<&'a HtmlElement> {
        if element.tag == md_tag::grid {
            // For grid: grid -> table -> table
            Self::find_table_in_grid(element)
        } else {
            // For m1table -> table
            Self::find_table_direct(element)
        }
    }

    fn find_table_in_grid<'a>(grid_element: &'a HtmlElement) -> Option<&'a HtmlElement> {
        for child in &grid_element.children {
            if let HtmlNode::Element(table_elem) = child {
                if table_elem.tag == md_tag::table {
                    // Find table tag within m1table
                    for inner_child in &table_elem.children {
                        if let HtmlNode::Element(inner) = inner_child {
                            if inner.tag == tag::table {
                                return Some(inner);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn find_table_direct<'a>(element: &'a HtmlElement) -> Option<&'a HtmlElement> {
        for child in &element.children {
            if let HtmlNode::Element(table_elem) = child {
                if table_elem.tag == tag::table {
                    return Some(table_elem);
                }
            }
        }
        None
    }

    // Extract table content from the table element
    fn extract_table_content(
        parser: &mut HtmlToAstParser,
        table: &HtmlElement,
        headers: &mut Vec<Vec<Node>>,
        rows: &mut Vec<Vec<Vec<Node>>>,
        is_header: &mut bool,
    ) -> Result<()> {
        // Process rows in the table
        for row_node in &table.children {
            if let HtmlNode::Element(row_elem) = row_node {
                if row_elem.tag == tag::tr {
                    let current_row =
                        Self::process_table_row(parser, row_elem, *is_header, headers)?;

                    // After the first row, treat remaining rows as data rows
                    if *is_header {
                        *is_header = false;
                    } else if !current_row.is_empty() {
                        rows.push(current_row);
                    }
                }
            }
        }
        Ok(())
    }

    fn process_table_row(
        parser: &mut HtmlToAstParser,
        row_elem: &HtmlElement,
        is_header: bool,
        headers: &mut Vec<Vec<Node>>,
    ) -> Result<Vec<Vec<Node>>> {
        let mut current_row = Vec::new();

        // Process cells in this row
        for cell_node in &row_elem.children {
            if let HtmlNode::Element(cell) = cell_node {
                if cell.tag == tag::td {
                    let mut cell_content = Vec::new();
                    parser.convert_children_into(&mut cell_content, cell)?;

                    // Add to appropriate section
                    if is_header {
                        headers.push(cell_content);
                    } else {
                        current_row.push(cell_content);
                    }
                }
            }
        }

        Ok(current_row)
    }

    /// Check if the table has complex cells (rowspan/colspan)
    fn table_has_complex_cells(table: &HtmlElement) -> bool {
        for row_node in &table.children {
            if let HtmlNode::Element(row_elem) = row_node {
                if row_elem.tag == tag::tr {
                    for cell_node in &row_elem.children {
                        if let HtmlNode::Element(cell) = cell_node {
                            if cell.tag == tag::td || cell.tag == tag::th {
                                if cell.attrs.iter().any(|(name, _)| {
                                    name.to_string().to_ascii_lowercase() == "colspan"
                                        || name.to_string().to_ascii_lowercase() == "rowspan"
                                }) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn create_table_node(
        headers: Vec<Vec<Node>>,
        rows: Vec<Vec<Vec<Node>>>,
    ) -> Result<Option<Node>> {
        // Create alignment array (default to None for all columns)
        let alignments = vec![TableAlignment::None; headers.len().max(1)];

        // If there is content, add the table to blocks
        if !headers.is_empty() || !rows.is_empty() {
            let flattened_headers = headers.into_iter().flatten().collect();
            let flattened_rows: Vec<_> = rows
                .into_iter()
                .map(|row| row.into_iter().flatten().collect())
                .collect();

            return Ok(Some(Node::Table {
                headers: flattened_headers,
                rows: flattened_rows,
                alignments,
            }));
        }

        Ok(None)
    }
}
