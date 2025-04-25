//! Markdown converter implementation

use base64::Engine;
use cmark_writer::ast::{
    BlockNode, HtmlAttribute, HtmlElement as CmarkHtmlElement, InlineNode, ListItem,
};
use cmark_writer::writer::CommonMarkWriter;
use cmark_writer::WriterOptions;
use ecow::EcoString;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::ListState;
use crate::tags::md_tag;
use crate::Result;
use crate::TypliteFeat;

/// Markdown converter implementation
#[derive(Clone, Default)]
pub struct MarkdownConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    blocks: Vec<BlockNode>,
    inline_buffer: Vec<InlineNode>,
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

impl MarkdownConverter {
    pub fn convert(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        self.blocks = Vec::new();
        self.inline_buffer = Vec::new();
        self.convert_element(root)?;
        self.flush_inline_buffer();

        let document = BlockNode::Document(self.blocks.clone());
        let mut writer = CommonMarkWriter::new();
        writer
            .write(&document.into_node())
            .expect("Failed to write document");
        w.push_str(&writer.into_string());

        Ok(())
    }

    fn flush_inline_buffer(&mut self) {
        if !self.inline_buffer.is_empty() {
            self.blocks.push(BlockNode::Paragraph(std::mem::take(
                &mut self.inline_buffer,
            )));
        }
    }

    fn flush_inline_buffer_as_block(
        &mut self,
        make_block: impl FnOnce(Vec<InlineNode>) -> BlockNode,
    ) {
        if !self.inline_buffer.is_empty() {
            self.blocks
                .push(make_block(std::mem::take(&mut self.inline_buffer)));
        }
    }

    fn convert_element(&mut self, element: &HtmlElement) -> Result<()> {
        // 调试使用，打印出当前元素的标签
        // println!("Converting element: {:?}", element.tag);
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
                self.flush_inline_buffer_as_block(|content| BlockNode::Heading {
                    level: attrs.level as u8 + 1,
                    content,
                });
                Ok(())
            }

            tag::ol => {
                self.flush_inline_buffer();
                let items = self.convert_list(element)?;
                self.blocks.push(BlockNode::OrderedList { start: 1, items });
                Ok(())
            }

            tag::ul => {
                self.flush_inline_buffer();
                let items = self.convert_list(element)?;
                self.blocks.push(BlockNode::UnorderedList(items));
                Ok(())
            }

            md_tag::raw => {
                let attrs = RawAttr::parse(&element.attrs)?;
                if attrs.block {
                    self.flush_inline_buffer();
                    self.blocks.push(BlockNode::CodeBlock {
                        language: Some(attrs.lang.into()),
                        content: attrs.text.into(),
                    });
                } else {
                    self.inline_buffer
                        .push(InlineNode::InlineCode(attrs.text.into()));
                }
                Ok(())
            }

            md_tag::quote => {
                self.flush_inline_buffer();
                self.convert_children(element)?;
                self.flush_inline_buffer_as_block(|content| {
                    BlockNode::BlockQuote(vec![BlockNode::Paragraph(content)])
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
                self.inline_buffer.push(InlineNode::Strong(content));
                Ok(())
            }

            tag::em | md_tag::emph => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(InlineNode::Emphasis(content));
                Ok(())
            }

            md_tag::strike => {
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(InlineNode::Strike(content));
                Ok(())
            }

            md_tag::link => {
                let attrs = LinkAttr::parse(&element.attrs)?;
                let mut content = Vec::new();
                self.convert_children_into(&mut content, element)?;
                self.inline_buffer.push(InlineNode::Link {
                    url: attrs.dest.into(),
                    title: None,
                    content,
                });
                Ok(())
            }

            md_tag::image => {
                let attrs = ImageAttr::parse(&element.attrs)?;
                let src = attrs.src.as_str();
                self.inline_buffer.push(InlineNode::Image {
                    url: src.to_string(),
                    title: None,
                    alt: attrs.alt.into(),
                });
                Ok(())
            }

            md_tag::linebreak => {
                self.inline_buffer.push(InlineNode::HardBreak);
                Ok(())
            }

            md_tag::table | md_tag::grid => {
                self.flush_inline_buffer();
                // Tables in CommonMark require headers, rows and alignments
                let mut headers = Vec::new();
                let mut rows = Vec::new();
                let mut current_row;
                let mut is_header = true;

                // Process table cells
                for child in &element.children {
                    if let HtmlNode::Element(row_elem) = child {
                        // Reset current row for each row element
                        current_row = Vec::new();

                        // Process cells in this row
                        for cell_node in &row_elem.children {
                            if let HtmlNode::Element(cell) = cell_node {
                                if cell.tag == md_tag::table_cell || cell.tag == md_tag::grid_cell {
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

                // Create alignments array (default to Center for all columns)
                let alignments = vec![cmark_writer::Alignment::None; headers.len().max(1)];

                // Add table to blocks if we have content
                if !headers.is_empty() || !rows.is_empty() {
                    let flattened_headers = headers.into_iter().flatten().collect();
                    let flattened_rows: Vec<_> = rows
                        .into_iter()
                        .map(|row| row.into_iter().flatten().collect())
                        .collect();
                    self.blocks.push(BlockNode::Table {
                        headers: flattened_headers,
                        rows: flattened_rows,
                        alignments,
                    });
                }

                Ok(())
            }

            md_tag::table_cell | md_tag::grid_cell => {
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

    fn convert_children(&mut self, element: &HtmlElement) -> Result<()> {
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => {
                    self.inline_buffer
                        .push(InlineNode::Text(text.as_str().to_string()));
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

    fn convert_children_into(
        &mut self,
        target: &mut Vec<InlineNode>,
        element: &HtmlElement,
    ) -> Result<()> {
        let prev_buffer = std::mem::take(&mut self.inline_buffer);
        self.convert_children(element)?;
        target.append(&mut self.inline_buffer);
        self.inline_buffer = prev_buffer;
        Ok(())
    }

    fn convert_list(&mut self, element: &HtmlElement) -> Result<Vec<ListItem>> {
        let mut all_items = Vec::new();
        let prev_buffer = std::mem::take(&mut self.inline_buffer);

        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    let mut item_content = Vec::new();

                    for li_child in &li.children {
                        match li_child {
                            HtmlNode::Text(text, _) => {
                                self.inline_buffer
                                    .push(InlineNode::Text(text.as_str().to_string()));
                            }
                            HtmlNode::Element(child_elem) => {
                                if child_elem.tag == tag::ul || child_elem.tag == tag::ol {
                                    if !self.inline_buffer.is_empty() {
                                        item_content.push(BlockNode::Paragraph(std::mem::take(
                                            &mut self.inline_buffer,
                                        )));
                                    }

                                    let items = self.convert_list(child_elem)?;
                                    if child_elem.tag == tag::ul {
                                        item_content.push(BlockNode::UnorderedList(items));
                                    } else {
                                        item_content
                                            .push(BlockNode::OrderedList { start: 1, items });
                                    }
                                } else {
                                    self.convert_element(child_elem)?;
                                }
                            }
                            _ => {}
                        }
                    }

                    if !self.inline_buffer.is_empty() {
                        item_content.push(BlockNode::Paragraph(std::mem::take(
                            &mut self.inline_buffer,
                        )));
                    }

                    if !item_content.is_empty() {
                        all_items.push(ListItem::Regular {
                            content: item_content,
                        });
                    }
                }
            }
        }

        self.inline_buffer = prev_buffer;
        Ok(all_items)
    }

    fn convert_frame(&self, frame: &Frame) -> InlineNode {
        let svg = typst_svg::svg_frame(frame);
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
        InlineNode::HtmlElement(CmarkHtmlElement {
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
