//! HTML table parsing module, processes the conversion of table elements

use cmark_writer::ast::Node;
use cmark_writer::gfm::TableAlignment;
use ecow::{EcoString, eco_format};
use typst::utils::PicoStr;
use typst_html::{HtmlElement, HtmlNode, tag};
use typst_syntax::Span;

use crate::Result;
use crate::attributes::{TableAttr, TypliteAttrsParser};
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

    pub fn is_structured_table(element: &HtmlElement) -> bool {
        element.children.iter().any(|child| {
            if let HtmlNode::Element(elem) = child {
                matches!(elem.tag, md_tag::cell | md_tag::header | md_tag::footer)
            } else {
                false
            }
        })
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
                        if state.has_data_rows {
                            parser.warn_at(
                                Some(element.span),
                                eco_format!(
                                    "table header appears after data rows; exported original HTML table"
                                ),
                            );
                            state.fallback_to_html = true;
                            return Ok(());
                        }
                        // Process header rows
                        Self::process_table_section(
                            parser,
                            element,
                            &mut state.headers,
                            &mut state.rows,
                            true,
                            &mut state.fallback_to_html,
                            state.has_data_rows,
                        )?;
                        state.is_header = false;
                    }
                    tag::tbody => {
                        // Mark that we have data rows
                        state.has_data_rows = true;
                        // Process body rows
                        Self::process_table_section(
                            parser,
                            element,
                            &mut state.headers,
                            &mut state.rows,
                            false,
                            &mut state.fallback_to_html,
                            state.has_data_rows,
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
                            state.has_data_rows,
                        )?;

                        // After the first row, treat remaining rows as data rows
                        if state.fallback_to_html {
                            return Ok(());
                        } else if state.is_header {
                            state.is_header = false;
                        } else if !current_row.is_empty() {
                            state.has_data_rows = true;
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
        has_data_rows: bool,
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
                    has_data_rows,
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
        has_data_rows: bool,
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

                // Check if this is a header cell appearing after data rows
                if cell.tag == tag::th && has_data_rows && !is_header {
                    parser.warn_at(
                            Some(cell.span),
                            eco_format!(
                                "table header cell appears after data rows; exported original HTML table"
                            ),
                        );
                    *fallback_to_html = true;
                    return Ok(Vec::new());
                }

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
    /// Track whether we have encountered any data rows
    pub has_data_rows: bool,
}

impl TableParseState {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            is_header: true,
            fallback_to_html: false,
            has_data_rows: false,
        }
    }
}

impl TableParser {
    /// Convert HTML table to CommonMark AST
    pub fn convert_table(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Option<Node>> {
        if element.tag == md_tag::table && TableStructureFinder::is_structured_table(element) {
            return Self::convert_structured_table(parser, element);
        }

        // Find the real table element
        let real_table_elem = TableStructureFinder::find_real_table_element(element);

        // Process the table (if found)
        if let Some(table) = real_table_elem {
            // Check if the table contains rowspan or colspan attributes
            // If it does, fall back to using HtmlElement
            if let Some(cell_span) = TableValidator::find_complex_cell(table) {
                parser.warn_at(
                    Some(cell_span),
                    eco_format!(
                        "table contains rowspan or colspan attributes; exported original HTML table"
                    ),
                );
                return parser.create_html_element(table).map(Some);
            }

            let mut state = TableParseState::new();
            TableContentExtractor::extract_table_content(parser, table, &mut state)?;

            if state.fallback_to_html {
                return parser.create_html_element(table).map(Some);
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

    fn convert_structured_table(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Option<Node>> {
        let attrs = TableAttr::parse(&element.attrs)?;
        let mut header_row: Vec<Node> = Vec::new();
        let mut extra_header_rows: Vec<Vec<Node>> = Vec::new();
        let mut footer_rows: Vec<Vec<Node>> = Vec::new();
        let mut body_cells: Vec<Node> = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(child_elem) = child {
                match child_elem.tag {
                    md_tag::header => {
                        let row = Self::convert_structured_row(parser, child_elem)?;
                        if header_row.is_empty() {
                            header_row = row;
                        } else {
                            extra_header_rows.push(row);
                        }
                    }
                    md_tag::footer => {
                        footer_rows.push(Self::convert_structured_row(parser, child_elem)?);
                    }
                    md_tag::cell => {
                        body_cells.push(Self::convert_structured_cell(parser, child_elem)?);
                    }
                    _ => {}
                }
            }
        }

        let columns = attrs.columns.unwrap_or(header_row.len().max(1)).max(1);

        let mut rows = Vec::new();
        if !body_cells.is_empty() {
            for chunk in body_cells.chunks(columns) {
                rows.push(chunk.to_vec());
            }
        }

        if header_row.is_empty() && !rows.is_empty() {
            header_row = rows.remove(0);
        }

        if !extra_header_rows.is_empty() {
            for row in extra_header_rows.into_iter().rev() {
                rows.insert(0, row);
            }
        }

        rows.extend(footer_rows);

        if header_row.is_empty() && rows.is_empty() {
            return Ok(None);
        }

        let alignment_len = header_row.len().max(1);

        Ok(Some(Node::Table {
            headers: header_row,
            alignments: vec![TableAlignment::None; alignment_len],
            rows,
        }))
    }

    fn convert_structured_row(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<Vec<Node>> {
        let mut cells = Vec::new();
        for child in &element.children {
            if let HtmlNode::Element(cell) = child
                && cell.tag == md_tag::cell
            {
                cells.push(Self::convert_structured_cell(parser, cell)?);
            }
        }
        Ok(cells)
    }

    fn convert_structured_cell(parser: &mut HtmlToAstParser, cell: &HtmlElement) -> Result<Node> {
        let (mut inline_nodes, block_nodes) = parser.capture_children(cell)?;
        if !block_nodes.is_empty() {
            inline_nodes.extend(block_nodes);
        }
        Ok(TableContentExtractor::merge_cell_content(inline_nodes))
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
        // table.cell has only one child (the body element)
        cell.children.first().and_then(|node| {
            if let HtmlNode::Element(elem) = node {
                if HtmlToAstParser::is_block_element(elem) {
                    return Some(elem);
                }
            }
            None
        })
    }

    fn resolve_span_and_tag(cell: &HtmlElement, block_elem: &HtmlElement) -> (Span, EcoString) {
        let span = if !block_elem.span.is_detached() {
            block_elem.span
        } else {
            cell.span
        };
        (span, block_elem.tag.resolve().to_string().into())
    }
}

/// Responsible for validating table structure and content.
pub struct TableValidator;

impl TableValidator {
    /// Check if the table has complex cells (rowspan/colspan), returns the span of the first complex cell
    pub fn find_complex_cell(table: &HtmlElement) -> Option<Span> {
        for child_node in &table.children {
            if let HtmlNode::Element(element) = child_node {
                match element.tag {
                    tag::thead | tag::tbody => {
                        // Check rows within thead/tbody
                        if let Some(span) = Self::check_section_for_complex_cells(element) {
                            return Some(span);
                        }
                    }
                    tag::tr => {
                        // Direct row
                        if let Some(span) = Self::check_row_for_complex_cells(element) {
                            return Some(span);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn check_section_for_complex_cells(section: &HtmlElement) -> Option<Span> {
        for row_node in &section.children {
            if let HtmlNode::Element(row_elem) = row_node
                && row_elem.tag == tag::tr
            {
                if let Some(span) = Self::check_row_for_complex_cells(row_elem) {
                    return Some(span);
                }
            }
        }
        None
    }

    fn check_row_for_complex_cells(row_elem: &HtmlElement) -> Option<Span> {
        for cell_node in &row_elem.children {
            if let HtmlNode::Element(cell) = cell_node
                && (cell.tag == tag::td || cell.tag == tag::th)
                && cell.attrs.0.iter().any(|(name, _)| {
                    let name = name.into_inner();
                    name == PicoStr::constant("colspan") || name == PicoStr::constant("rowspan")
                })
            {
                return Some(cell.span);
            }
        }
        None
    }
}
