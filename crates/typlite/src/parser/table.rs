//! HTML table parsing module, processes the conversion of table elements

use cmark_writer::HtmlWriter;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use cmark_writer::gfm::TableAlignment;
use ecow::{EcoString, eco_format};
use typst::html::{HtmlElement, HtmlNode, tag};
use typst::utils::PicoStr;

use crate::Result;
use crate::common::InlineNode;
use crate::tags::md_tag;

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
                return parser.create_html_element(table).map(Some);
            }

            let mut headers = Vec::new();
            let mut rows = Vec::new();
            let mut is_header = true;

            let mut fallback_to_html = false;
            Self::extract_table_content(
                parser,
                table,
                &mut headers,
                &mut rows,
                &mut is_header,
                &mut fallback_to_html,
            )?;

            if fallback_to_html {
                eprintln!(
                    "[typlite] warning: block content detected inside table cell; exporting original HTML table"
                );
                let html = Self::serialize_html_element(parser, table)?;
                let html = eco_format!(
                    "<!-- typlite warning: block content detected inside table cell; exported original HTML table -->\n{}",
                    html
                );
                return Ok(Some(Node::HtmlBlock(html)));
            }

            return Self::create_table_node(headers, rows);
        }

        Ok(None)
    }

    /// Find the real table element in the HTML structure
    fn find_real_table_element(element: &HtmlElement) -> Option<&HtmlElement> {
        if element.tag == md_tag::grid {
            // For grid: grid -> table -> table
            Self::find_table_in_grid(element)
        } else {
            // For m1table -> table
            Self::find_table_direct(element)
        }
    }

    fn find_table_in_grid(grid_element: &HtmlElement) -> Option<&HtmlElement> {
        for child in &grid_element.children {
            if let HtmlNode::Element(table_elem) = child
                && table_elem.tag == md_tag::table
            {
                // Find table tag within m1table
                for inner_child in &table_elem.children {
                    if let HtmlNode::Element(inner) = inner_child
                        && inner.tag == tag::table
                    {
                        return Some(inner);
                    }
                }
            }
        }
        None
    }

    fn find_table_direct(element: &HtmlElement) -> Option<&HtmlElement> {
        for child in &element.children {
            if let HtmlNode::Element(table_elem) = child
                && table_elem.tag == tag::table
            {
                return Some(table_elem);
            }
        }
        None
    }

    // Extract table content from the table element
    fn extract_table_content(
        parser: &mut HtmlToAstParser,
        table: &HtmlElement,
        headers: &mut Vec<Node>,
        rows: &mut Vec<Vec<Node>>,
        is_header: &mut bool,
        fallback_to_html: &mut bool,
    ) -> Result<()> {
        if *fallback_to_html {
            return Ok(());
        }
        // Process table structure (direct rows or thead/tbody)
        for child_node in &table.children {
            if let HtmlNode::Element(element) = child_node {
                match element.tag {
                    tag::thead => {
                        // Process header rows
                        Self::process_table_section(
                            parser,
                            element,
                            headers,
                            rows,
                            true,
                            fallback_to_html,
                        )?;
                        *is_header = false;
                    }
                    tag::tbody => {
                        // Process body rows
                        Self::process_table_section(
                            parser,
                            element,
                            headers,
                            rows,
                            false,
                            fallback_to_html,
                        )?;
                    }
                    tag::tr => {
                        // Direct row (no thead/tbody structure)
                        let current_row = Self::process_table_row(
                            parser,
                            element,
                            *is_header,
                            headers,
                            fallback_to_html,
                        )?;

                        // After the first row, treat remaining rows as data rows
                        if *fallback_to_html {
                            return Ok(());
                        } else if *is_header {
                            *is_header = false;
                        } else if !current_row.is_empty() {
                            rows.push(current_row);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn process_table_section(
        parser: &mut HtmlToAstParser,
        section: &HtmlElement,
        headers: &mut Vec<Node>,
        rows: &mut Vec<Vec<Node>>,
        is_header_section: bool,
        fallback_to_html: &mut bool,
    ) -> Result<()> {
        if *fallback_to_html {
            return Ok(());
        }
        for row_node in &section.children {
            if let HtmlNode::Element(row_elem) = row_node
                && row_elem.tag == tag::tr
            {
                let current_row = Self::process_table_row(
                    parser,
                    row_elem,
                    is_header_section,
                    headers,
                    fallback_to_html,
                )?;

                if *fallback_to_html {
                    return Ok(());
                }

                if !is_header_section && !current_row.is_empty() {
                    rows.push(current_row);
                }
            }
        }
        Ok(())
    }

    fn process_table_row(
        parser: &mut HtmlToAstParser,
        row_elem: &HtmlElement,
        is_header: bool,
        headers: &mut Vec<Node>,
        fallback_to_html: &mut bool,
    ) -> Result<Vec<Node>> {
        if *fallback_to_html {
            return Ok(Vec::new());
        }
        let mut current_row = Vec::new();

        // Process cells in this row
        for cell_node in &row_elem.children {
            if let HtmlNode::Element(cell) = cell_node
                && (cell.tag == tag::td || cell.tag == tag::th)
            {
                let (cell_content, block_content) = parser.capture_children(cell)?;

                if !block_content.is_empty() {
                    *fallback_to_html = true;
                    return Ok(Vec::new());
                }

                // Merge cell content into a single node
                let merged_cell = Self::merge_cell_content(cell_content);

                // Add to appropriate section
                if is_header || cell.tag == tag::th {
                    headers.push(merged_cell);
                } else {
                    current_row.push(merged_cell);
                }
            }
        }

        Ok(current_row)
    }

    /// Merge cell content nodes into a single node
    fn merge_cell_content(content: Vec<Node>) -> Node {
        match content.len() {
            0 => Node::Text(EcoString::new()),
            1 => content.into_iter().next().unwrap(),
            _ => Node::Custom(Box::new(InlineNode { content })),
        }
    }

    /// Check if the table has complex cells (rowspan/colspan)
    fn table_has_complex_cells(table: &HtmlElement) -> bool {
        for child_node in &table.children {
            if let HtmlNode::Element(element) = child_node {
                match element.tag {
                    tag::thead | tag::tbody => {
                        // Check rows within thead/tbody
                        if Self::check_section_for_complex_cells(element) {
                            return true;
                        }
                    }
                    tag::tr => {
                        // Direct row
                        if Self::check_row_for_complex_cells(element) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn check_section_for_complex_cells(section: &HtmlElement) -> bool {
        for row_node in &section.children {
            if let HtmlNode::Element(row_elem) = row_node
                && row_elem.tag == tag::tr
                && Self::check_row_for_complex_cells(row_elem)
            {
                return true;
            }
        }
        false
    }

    fn check_row_for_complex_cells(row_elem: &HtmlElement) -> bool {
        for cell_node in &row_elem.children {
            if let HtmlNode::Element(cell) = cell_node
                && (cell.tag == tag::td || cell.tag == tag::th)
                && cell.attrs.0.iter().any(|(name, _)| {
                    let name = name.into_inner();
                    name == PicoStr::constant("colspan") || name == PicoStr::constant("rowspan")
                })
            {
                return true;
            }
        }
        false
    }

    fn create_table_node(headers: Vec<Node>, rows: Vec<Vec<Node>>) -> Result<Option<Node>> {
        // Create alignment array (default to None for all columns)
        let alignments = vec![TableAlignment::None; headers.len().max(1)];

        // If there is content, add the table to blocks
        if !headers.is_empty() || !rows.is_empty() {
            return Ok(Some(Node::Table {
                headers,
                rows,
                alignments,
            }));
        }

        Ok(None)
    }

    fn serialize_html_element(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<EcoString> {
        let node = Node::HtmlElement(Self::build_html_element(parser, element));
        let mut writer = HtmlWriter::new();
        writer.write_node(&node).map_err(|err| err.to_string())?;
        Ok(writer.into_string())
    }

    fn build_html_element(parser: &mut HtmlToAstParser, element: &HtmlElement) -> CmarkHtmlElement {
        let attributes = element
            .attrs
            .0
            .iter()
            .map(|(name, value)| HtmlAttribute {
                name: name.resolve().to_string().into(),
                value: value.clone(),
            })
            .collect();

        let mut children = Vec::new();
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => children.push(Node::Text(text.clone())),
                HtmlNode::Element(elem) => {
                    children.push(Node::HtmlElement(Self::build_html_element(parser, elem)))
                }
                HtmlNode::Frame(frame) => children.push(parser.convert_frame(frame)),
                HtmlNode::Tag(_) => {}
            }
        }

        CmarkHtmlElement {
            tag: element.tag.resolve().to_string().into(),
            attributes,
            children,
            self_closing: element.children.is_empty(),
        }
    }
}
