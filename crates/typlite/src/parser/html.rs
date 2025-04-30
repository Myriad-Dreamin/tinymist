//! HTML to AST parser implementation

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, ListItem, Node};
use cmark_writer::gfm::TableAlignment;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{
    FigureAttr, HeadingAttr, ImageAttr, LinkAttr, ListItemAttr, RawAttr, TypliteAttrsParser,
};
use crate::common::{FigureNode, ListState};
use crate::tags::md_tag;
use crate::Result;
use crate::TypliteFeat;

use super::Parser;
use std::sync::atomic::{AtomicUsize, Ordering};

/// HTML to AST parser implementation
pub struct HtmlToAstParser {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub blocks: Vec<Node>,
    pub inline_buffer: Vec<Node>,
}

impl HtmlToAstParser {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            blocks: Vec::new(),
            inline_buffer: Vec::new(),
        }
    }

    pub fn convert_element(&mut self, element: &HtmlElement) -> Result<()> {
        match element.tag {
            tag::head => Ok(()),

            tag::html | tag::body | md_tag::doc => {
                self.convert_children(element)?;
                Ok(())
            }

            md_tag::parbreak => {
                self.flush_inline_buffer();
                Ok(())
            }

            md_tag::heading => {
                self.flush_inline_buffer();
                let attrs = HeadingAttr::parse(&element.attrs)?;
                self.convert_children(element)?;
                self.flush_inline_buffer_as_block(|content| {
                    Node::heading(attrs.level as u8 + 1, content)
                });
                Ok(())
            }

            tag::ol => {
                self.flush_inline_buffer();
                let items = self.convert_list(element)?;
                self.blocks.push(Node::OrderedList { start: 1, items });
                Ok(())
            }

            tag::ul => {
                self.flush_inline_buffer();
                let items = self.convert_list(element)?;
                self.blocks.push(Node::UnorderedList(items));
                Ok(())
            }

            md_tag::raw => {
                let attrs = RawAttr::parse(&element.attrs)?;
                if attrs.block {
                    self.flush_inline_buffer();
                    self.blocks
                        .push(Node::code_block(Some(attrs.lang.into()), attrs.text.into()));
                } else {
                    self.inline_buffer.push(Node::InlineCode(attrs.text.into()));
                }
                Ok(())
            }

            md_tag::quote => {
                self.flush_inline_buffer();
                self.convert_children(element)?;
                self.flush_inline_buffer_as_block(|content| {
                    Node::BlockQuote(vec![Node::Paragraph(content)])
                });
                Ok(())
            }

            md_tag::figure => {
                self.flush_inline_buffer();

                // Parse figure attributes to extract caption
                let attrs = FigureAttr::parse(&element.attrs)?;
                let caption = attrs.caption.to_string();

                // Find the image and body content
                let mut body_content = Vec::new();
                self.convert_children_into(&mut body_content, element)?;
                let body = Box::new(Node::Paragraph(body_content));

                // Create a figure node using the common definition
                self.blocks
                    .push(Node::Custom(Box::new(FigureNode { body, caption })));

                Ok(())
            }

            tag::p | tag::span => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::strong | md_tag::strong => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(Node::Strong(content));
                Ok(())
            }

            tag::em | md_tag::emph => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(Node::Emphasis(content));
                Ok(())
            }

            md_tag::highlight => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(Node::HtmlElement(CmarkHtmlElement {
                    tag: "mark".to_string(),
                    attributes: vec![],
                    children: content,
                    self_closing: false,
                }));
                Ok(())
            }

            md_tag::strike => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(Node::Strikethrough(content));
                Ok(())
            }

            md_tag::link => {
                let attrs = LinkAttr::parse(&element.attrs)?;
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(Node::Link {
                    url: attrs.dest.into(),
                    title: None,
                    content,
                });
                Ok(())
            }

            md_tag::image => {
                let attrs = ImageAttr::parse(&element.attrs)?;
                let src = attrs.src.as_str();
                self.inline_buffer.push(Node::Image {
                    url: src.to_string(),
                    title: None,
                    alt: vec![Node::Text(attrs.alt.into())],
                });
                Ok(())
            }

            md_tag::linebreak => {
                self.inline_buffer.push(Node::HardBreak);
                Ok(())
            }

            md_tag::table | md_tag::grid => {
                self.flush_inline_buffer();
                let table = self.convert_table(element)?;
                if let Some(table) = table {
                    self.blocks.push(table);
                }
                Ok(())
            }

            md_tag::math_equation_inline | md_tag::math_equation_block => {
                if element.tag == md_tag::math_equation_block {
                    self.flush_inline_buffer();
                }
                self.convert_children(element)?;
                if element.tag == md_tag::math_equation_block {
                    self.flush_inline_buffer();
                }
                Ok(())
            }

            _ => {
                let tag_name = element.tag.resolve().to_string();

                if !tag_name.starts_with("m1") {
                    let html_element = self.create_html_element(element)?;
                    self.inline_buffer.push(html_element);
                } else {
                    self.convert_children(element)?;
                }
                Ok(())
            }
        }
    }

    /// Create a CommonMark HTML element from the given HTML element    
    fn create_html_element(&mut self, element: &HtmlElement) -> Result<Node> {
        let attributes = element
            .attrs
            .iter()
            .map(|(name, value)| HtmlAttribute {
                name: name.to_string(),
                value: value.to_string(),
            })
            .collect();

        let mut children = Vec::new();
        self.convert_children_into(&mut children, element)?;

        Ok(Node::HtmlElement(CmarkHtmlElement {
            tag: element.tag.resolve().to_string(),
            attributes,
            children,
            self_closing: element.children.is_empty(),
        }))
    }

    pub fn flush_inline_buffer(&mut self) {
        if !self.inline_buffer.is_empty() {
            self.blocks
                .push(Node::Paragraph(std::mem::take(&mut self.inline_buffer)));
        }
    }

    pub fn flush_inline_buffer_as_block(&mut self, make_block: impl FnOnce(Vec<Node>) -> Node) {
        if !self.inline_buffer.is_empty() {
            self.blocks
                .push(make_block(std::mem::take(&mut self.inline_buffer)));
        }
    }

    pub fn convert_children(&mut self, element: &HtmlElement) -> Result<()> {
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => {
                    self.inline_buffer
                        .push(Node::Text(text.as_str().to_string()));
                }
                HtmlNode::Element(element) => {
                    self.convert_element(element)?;
                }
                HtmlNode::Frame(frame) => {
                    self.inline_buffer.push(self.convert_frame(frame));
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn convert_children_into(
        &mut self,
        target: &mut Vec<Node>,
        element: &HtmlElement,
    ) -> Result<()> {
        let prev_buffer = std::mem::take(&mut self.inline_buffer);
        self.convert_children(element)?;
        target.append(&mut self.inline_buffer);
        self.inline_buffer = prev_buffer;
        Ok(())
    }

    pub fn convert_list(&mut self, element: &HtmlElement) -> Result<Vec<ListItem>> {
        let mut all_items = Vec::new();
        let prev_buffer = std::mem::take(&mut self.inline_buffer);
        let is_ordered = element.tag == tag::ol;

        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    let attrs = ListItemAttr::parse(&li.attrs)?;

                    let mut item_content = Vec::new();

                    for li_child in &li.children {
                        match li_child {
                            HtmlNode::Text(text, _) => {
                                self.inline_buffer
                                    .push(Node::Text(text.as_str().to_string()));
                            }
                            HtmlNode::Element(child_elem) => {
                                if child_elem.tag == tag::ul || child_elem.tag == tag::ol {
                                    if !self.inline_buffer.is_empty() {
                                        item_content.push(Node::Paragraph(std::mem::take(
                                            &mut self.inline_buffer,
                                        )));
                                    }

                                    let items = self.convert_list(child_elem)?;
                                    if child_elem.tag == tag::ul {
                                        item_content.push(Node::UnorderedList(items));
                                    } else {
                                        item_content.push(Node::OrderedList { start: 1, items });
                                    }
                                } else {
                                    self.convert_element(child_elem)?;
                                }
                            }
                            _ => {}
                        }
                    }

                    if !self.inline_buffer.is_empty() {
                        item_content.push(Node::Paragraph(std::mem::take(&mut self.inline_buffer)));
                    }

                    if !item_content.is_empty() {
                        if is_ordered {
                            all_items.push(ListItem::Ordered {
                                number: attrs.value,
                                content: item_content,
                            });
                        } else {
                            all_items.push(ListItem::Unordered {
                                content: item_content,
                            });
                        }
                    }
                }
            }
        }

        self.inline_buffer = prev_buffer;
        Ok(all_items)
    }

    /// Convert HTML table to CommonMark AST
    pub fn convert_table(&mut self, element: &HtmlElement) -> Result<Option<Node>> {
        // Find real table element
        let real_table_elem = self.find_real_table_element(element);

        // Process table if the real table element was found
        if let Some(table) = real_table_elem {
            // Check if the table contains rowspan or colspan attributes
            // If so, fall back to using HtmlElement
            if self.table_has_complex_cells(table) {
                if let Ok(html_node) = self.create_html_element(table) {
                    return Ok(Some(html_node));
                }
                return Ok(None);
            }

            let mut headers = Vec::new();
            let mut rows = Vec::new();
            let mut is_header = true;

            self.extract_table_content(table, &mut headers, &mut rows, &mut is_header)?;
            return self.create_table_node(headers, rows);
        }

        Ok(None)
    }

    /// Find the real table element in the HTML structure   
    fn find_real_table_element<'a>(&self, element: &'a HtmlElement) -> Option<&'a HtmlElement> {
        if element.tag == md_tag::grid {
            // For grid: grid -> table -> table
            self.find_table_in_grid(element)
        } else {
            // For m1table -> table
            self.find_table_direct(element)
        }
    }

    fn find_table_in_grid<'a>(&self, grid_element: &'a HtmlElement) -> Option<&'a HtmlElement> {
        for child in &grid_element.children {
            if let HtmlNode::Element(table_elem) = child {
                if table_elem.tag == md_tag::table {
                    // Find table tag inside m1table
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

    fn find_table_direct<'a>(&self, element: &'a HtmlElement) -> Option<&'a HtmlElement> {
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
        &mut self,
        table: &HtmlElement,
        headers: &mut Vec<Vec<Node>>,
        rows: &mut Vec<Vec<Vec<Node>>>,
        is_header: &mut bool,
    ) -> Result<()> {
        // Process rows in table
        for row_node in &table.children {
            if let HtmlNode::Element(row_elem) = row_node {
                if row_elem.tag == tag::tr {
                    let current_row = self.process_table_row(row_elem, *is_header, headers)?;

                    // After first row, treat remaining rows as data rows
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
        &mut self,
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
                    self.convert_children_into(&mut cell_content, cell)?;

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
    fn table_has_complex_cells(&self, table: &HtmlElement) -> bool {
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
        &self,
        headers: Vec<Vec<Node>>,
        rows: Vec<Vec<Vec<Node>>>,
    ) -> Result<Option<Node>> {
        // Create alignments array (default to None for all columns)
        let alignments = vec![TableAlignment::None; headers.len().max(1)];

        // Add table to blocks if we have content
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

    pub fn convert_frame(&self, frame: &Frame) -> Node {
        let svg = typst_svg::svg_frame(frame);
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());

        if let Some(assets_path) = &self.feat.assets_path {
            // Use a unique static counter for filenames
            static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);
            let file_id = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
            let file_name = format!("frame_{}.svg", file_id);
            let file_path = assets_path.join(&file_name);

            if let Err(e) = std::fs::write(&file_path, svg.as_bytes()) {
                if self.feat.soft_error {
                    return self.create_embedded_frame(&data);
                } else {
                    // Otherwise, construct an error node
                    return Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error writing frame to file: {}", e))],
                        self_closing: false,
                    });
                }
            }

            return Node::Custom(Box::new(crate::common::ExternalFrameNode {
                file_path,
                alt_text: "typst-frame".to_string(),
                svg_data: data,
            }));
        }

        // If no external assets path specified, fall back to embedded mode
        self.create_embedded_frame(&data)
    }

    fn create_embedded_frame(&self, data: &str) -> Node {
        Node::HtmlElement(CmarkHtmlElement {
            tag: "img".to_string(),
            attributes: vec![
                HtmlAttribute {
                    name: "alt".to_string(),
                    value: "typst-block".to_string(),
                },
                HtmlAttribute {
                    name: "src".to_string(),
                    value: format!("data:image/svg+xml;base64,{data}"),
                },
            ],
            children: vec![],
            self_closing: true,
        })
    }
}

impl Parser for HtmlToAstParser {
    fn parse(&self, root: &HtmlElement) -> Result<Node> {
        let mut parser = Self {
            feat: self.feat.clone(),
            list_state: self.list_state,
            blocks: Vec::new(),
            inline_buffer: Vec::new(),
        };

        parser.convert_element(root)?;
        parser.flush_inline_buffer();

        Ok(Node::Document(parser.blocks))
    }
}
