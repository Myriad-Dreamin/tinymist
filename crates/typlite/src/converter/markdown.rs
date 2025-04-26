//! Markdown converter implementation

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, ListItem, Node};
use cmark_writer::writer::CommonMarkWriter;
use ecow::EcoString;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::{FormatWriter, ListState};
use crate::tags::md_tag;
use crate::Result;
use crate::TypliteFeat;

/// Markdown converter implementation
#[derive(Clone, Default)]
pub struct MarkdownConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub blocks: Vec<Node>,
    pub inline_buffer: Vec<Node>,
}

impl MarkdownConverter {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            blocks: Vec::new(),
            inline_buffer: Vec::new(),
        }
    }
}

// Maintain original parsing logic but expose more internal methods for shared parser use
impl MarkdownConverter {
    pub fn convert(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        self.blocks = Vec::new();
        self.inline_buffer = Vec::new();
        self.convert_element(root)?;
        self.flush_inline_buffer();

        let document = Node::Document(self.blocks.clone());

        // Use MarkdownWriter for output
        let mut writer = MarkdownWriter::new();
        writer.write_eco(&document, w)?;

        Ok(())
    }

    pub fn convert_element(&mut self, element: &HtmlElement) -> Result<()> {
        match element.tag {
            tag::head => Ok(()),

            tag::html | tag::body | md_tag::doc => {
                for child in &element.children {
                    if let HtmlNode::Element(child_elem) = child {
                        self.convert_element(child_elem)?;
                    }
                }
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
                self.flush_inline_buffer_as_block(|content| Node::Heading {
                    level: attrs.level as u8 + 1,
                    content,
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
                    self.blocks.push(Node::CodeBlock {
                        language: Some(attrs.lang.into()),
                        content: attrs.text.into(),
                    });
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
                self.inline_buffer.push(Node::Strike(content));
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
                // Tables in CommonMark require headers, rows and alignments
                let mut headers = Vec::new();
                let mut rows = Vec::new();
                let mut current_row;
                let mut is_header = true;

                // Find real table element - either directly in m1table or inside m1grid/m1table
                let real_table_elem = if element.tag == md_tag::grid {
                    // For grid: grid -> table -> table
                    let mut inner_table = None;

                    for child in &element.children {
                        if let HtmlNode::Element(table_elem) = child {
                            if table_elem.tag == md_tag::table {
                                // Find table tag inside m1table
                                for inner_child in &table_elem.children {
                                    if let HtmlNode::Element(inner) = inner_child {
                                        if inner.tag == tag::table {
                                            inner_table = Some(inner);
                                            break;
                                        }
                                    }
                                }

                                if inner_table.is_some() {
                                    break;
                                }
                            }
                        }
                    }

                    inner_table
                } else {
                    // For m1table -> table
                    let mut direct_table = None;

                    for child in &element.children {
                        if let HtmlNode::Element(table_elem) = child {
                            if table_elem.tag == tag::table {
                                direct_table = Some(table_elem);
                                break;
                            }
                        }
                    }

                    direct_table
                };

                // Process table rows and cells if the real table element was found
                if let Some(table) = real_table_elem {
                    // Process rows in table
                    for row_node in &table.children {
                        if let HtmlNode::Element(row_elem) = row_node {
                            if row_elem.tag == tag::tr {
                                // Reset current row for each row element
                                current_row = Vec::new();

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

                                // After first row, treat remaining rows as data rows
                                if is_header {
                                    is_header = false;
                                } else if !current_row.is_empty() {
                                    rows.push(current_row);
                                }
                            }
                        }
                    }
                }

                // Create alignments array (default to Center for all columns)
                let alignments = vec![cmark_writer::Alignment::None; headers.len().max(1)];

                // Add table to blocks if we have content
                if !headers.is_empty() || !rows.is_empty() {
                    let flattened_headers = headers.into_iter().flatten().collect();
                    let flattened_rows: Vec<_> = rows
                        .into_iter()
                        .map(|row| row.into_iter().flatten().collect())
                        .collect();
                    self.blocks.push(Node::Table {
                        headers: flattened_headers,
                        rows: flattened_rows,
                        alignments,
                    });
                }

                Ok(())
            }

            tag::td | tag::tr => {
                self.convert_children(element)?;
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
                self.convert_children(element)?;
                Ok(())
            }
        }
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
                    let attrs = crate::attributes::ListItemAttr::parse(&li.attrs)?;

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

    pub fn convert_frame(&self, frame: &Frame) -> Node {
        let svg = typst_svg::svg_frame(frame);
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
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

/// Markdown writer implementation that uses cmark-writer to write documents
#[derive(Default)]
pub struct MarkdownWriter {}

impl MarkdownWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl FormatWriter for MarkdownWriter {
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()> {
        let mut writer = CommonMarkWriter::new();
        writer.write(document).expect("Failed to write document");
        output.push_str(&writer.into_string());
        Ok(())
    }

    fn write_vec(&mut self, _document: &Node) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}
