//! HTML table parsing module, processes the conversion of table elements

use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use cmark_writer::gfm::TableAlignment;
use cmark_writer::{HtmlWriteError, HtmlWriter};
use ecow::{EcoString, eco_format};
use typst::html::{HtmlElement, HtmlNode, tag};
use typst::utils::PicoStr;
use typst_syntax::Span;

use crate::Result;
use crate::common::InlineNode;
use crate::tags::md_tag;

use super::core::HtmlToAstParser;

/// Responsible for finding HTML table elements in the DOM structure.
pub struct TableStructureFinder;

impl TableStructureFinder {
    /// Find the real table element in the HTML structure
    pub fn find_real_table_element(element: &HtmlElement) -> Option<&HtmlElement> {
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
}

/// Responsible for extracting and processing table content from HTML elements.
pub struct TableContentExtractor;

impl TableContentExtractor {
    // Extract table content from the table element
    pub fn extract_table_content(
        parser: &mut HtmlToAstParser,
        table: &HtmlElement,
        state: &mut TableParseState,
    ) -> Result<()> {
        if state.fallback_to_html {
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
                            &mut state.headers,
                            &mut state.rows,
                            true,
                            &mut state.fallback_to_html,
                        )?;
                        state.is_header = false;
                    }
                    tag::tbody => {
                        // Process body rows
                        Self::process_table_section(
                            parser,
                            element,
                            &mut state.headers,
                            &mut state.rows,
                            false,
                            &mut state.fallback_to_html,
                        )?;
                    }
                    tag::tr => {
                        // Direct row (no thead/tbody structure)
                        let current_row = Self::process_table_row(
                            parser,
                            element,
                            state.is_header,
                            &mut state.headers,
                            &mut state.fallback_to_html,
                        )?;

                        // After the first row, treat remaining rows as data rows
                        if state.fallback_to_html {
                            return Ok(());
                        } else if state.is_header {
                            state.is_header = false;
                        } else if !current_row.is_empty() {
                            state.rows.push(current_row);
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
                    parser.warn_at(
                        Some(cell.span),
                        eco_format!(
                            "block content detected inside table cell; exported original HTML table"
                        ),
                    );
                    *fallback_to_html = true;
                    TableSpanResolver::emit_table_fallback_warning(parser, cell);
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
    pub fn merge_cell_content(content: Vec<Node>) -> Node {
        match content.len() {
            0 => Node::Text(EcoString::new()),
            1 => content.into_iter().next().unwrap(),
            _ => Node::Custom(Box::new(InlineNode { content })),
        }
    }
}

/// Table parser
pub struct TableParser;

/// State for table parsing operations
pub struct TableParseState {
    pub headers: Vec<Node>,
    pub rows: Vec<Vec<Node>>,
    pub is_header: bool,
    pub fallback_to_html: bool,
}

impl TableParseState {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            is_header: true,
            fallback_to_html: false,
        }
    }
}

impl TableParser {
    /// Convert HTML table to CommonMark AST
    pub fn convert_table(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Option<Node>> {
        // Find the real table element
        let real_table_elem = TableStructureFinder::find_real_table_element(element);

        // Process the table (if found)
        if let Some(table) = real_table_elem {
            // Check if the table contains rowspan or colspan attributes
            // If it does, fall back to using HtmlElement
            if TableValidator::table_has_complex_cells(table) {
                TableValidator::emit_complex_cells_warning(parser, table);
                return parser.create_html_element(table).map(Some);
            }

            let mut state = TableParseState::new();
            TableContentExtractor::extract_table_content(parser, table, &mut state)?;

            if state.fallback_to_html {
                let html = TableSerializer::serialize_html_element(parser, table)
                    .map_err(|e| e.to_string())?;
                return Ok(Some(Node::HtmlBlock(html)));
            }

            return Self::create_table_node(state.headers, state.rows);
        }

        Ok(None)
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
}

/// Responsible for resolving spans and emitting warnings for table parsing.
pub struct TableSpanResolver;

impl TableSpanResolver {
    pub fn emit_table_fallback_warning(parser: &mut HtmlToAstParser, cell: &HtmlElement) {
        if let Some(block_elem) = Self::find_block_child(cell) {
            let (span, tag_name) = Self::resolve_span_and_tag(cell, block_elem);
            parser.warn_at(
                Some(span),
                eco_format!(
                    "block element `<{tag_name}>` detected inside table cell; exported original HTML table"
                ),
            );
        } else {
            parser.warn_at(
                Some(cell.span),
                eco_format!(
                    "block content detected inside table cell; exported original HTML table"
                ),
            );
        }
    }

    fn find_block_child(cell: &HtmlElement) -> Option<&HtmlElement> {
        Self::find_block_child_in_nodes(&cell.children)
    }

    fn find_block_child_in_nodes(nodes: &[HtmlNode]) -> Option<&HtmlElement> {
        for node in nodes {
            if let HtmlNode::Element(elem) = node {
                if HtmlToAstParser::is_block_element(elem) {
                    return Some(elem);
                }

                if let Some(found) = Self::find_block_child_in_nodes(&elem.children) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn resolve_span_and_tag(cell: &HtmlElement, block_elem: &HtmlElement) -> (Span, EcoString) {
        if let Some(elem) = Self::find_element_with_span(block_elem) {
            return (elem.span, elem.tag.resolve().to_string().into());
        }

        if !cell.span.is_detached() {
            return (cell.span, cell.tag.resolve().to_string().into());
        }

        if !block_elem.span.is_detached() {
            return (block_elem.span, block_elem.tag.resolve().to_string().into());
        }

        if let Some(span) = Self::find_descendant_text_span(&block_elem.children) {
            return (span, block_elem.tag.resolve().to_string().into());
        }

        (block_elem.span, block_elem.tag.resolve().to_string().into())
    }

    fn find_element_with_span(element: &HtmlElement) -> Option<&HtmlElement> {
        for node in &element.children {
            if let HtmlNode::Element(child) = node {
                if !child.span.is_detached() {
                    return Some(child);
                }
            }
        }

        for node in &element.children {
            if let HtmlNode::Element(child) = node {
                if let Some(found) = Self::find_element_with_span(child) {
                    return Some(found);
                }
            }
        }

        None
    }

    fn find_descendant_text_span(nodes: &[HtmlNode]) -> Option<Span> {
        for node in nodes {
            match node {
                HtmlNode::Text(_, span) if !span.is_detached() => return Some(*span),
                HtmlNode::Element(elem) => {
                    if let Some(span) = Self::find_descendant_text_span(&elem.children) {
                        return Some(span);
                    }
                }
                HtmlNode::Frame(_) | HtmlNode::Tag(_) | HtmlNode::Text(_, _) => {}
            }
        }
        None
    }
}

/// Responsible for validating table structure and content.
pub struct TableValidator;

impl TableValidator {
    /// Check if the table has complex cells (rowspan/colspan)
    pub fn table_has_complex_cells(table: &HtmlElement) -> bool {
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

    /// Emit warning for complex table cells
    pub fn emit_complex_cells_warning(parser: &mut HtmlToAstParser, table: &HtmlElement) {
        parser.warn_at(
            Some(table.span),
            eco_format!(
                "table contains rowspan or colspan attributes; exported original HTML table"
            ),
        );
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
}

/// Responsible for serializing HTML elements back to HTML strings.
pub struct TableSerializer;

impl TableSerializer {
    /// Serialize HTML element to HTML string
    pub fn serialize_html_element(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<EcoString, HtmlWriteError> {
        let node = Node::HtmlElement(Self::build_html_element(parser, element)?);
        let mut writer = HtmlWriter::new();
        writer.write_node(&node)?;
        writer.into_string()
    }

    fn build_html_element(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<CmarkHtmlElement, HtmlWriteError> {
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
                    children.push(Node::HtmlElement(Self::build_html_element(parser, elem)?))
                }
                HtmlNode::Frame(frame) => children.push(parser.convert_frame(frame)),
                HtmlNode::Tag(_) => {}
            }
        }

        Ok(CmarkHtmlElement {
            tag: element.tag.resolve().to_string().into(),
            attributes,
            children,
            self_closing: element.children.is_empty(),
        })
    }
}
