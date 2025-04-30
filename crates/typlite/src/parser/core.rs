//! HTML parser core, containing main structures and general parsing logic

use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use cmark_writer::CustomNode;
use typst::html::{tag, HtmlElement, HtmlNode};

use crate::attributes::{HeadingAttr, RawAttr, TypliteAttrsParser};
use crate::common::ListState;
use crate::tags::md_tag;
use crate::Result;
use crate::TypliteFeat;

use super::{inline::InlineParser, list::ListParser, media::MediaParser, table::TableParser};

/// HTML to AST parser implementation
pub struct HtmlToAstParser {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    pub blocks: Vec<Node>,
    pub inline_buffer: Vec<Node>,
}

impl HtmlToAstParser {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_level: 0,
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
                self.list_level += 1;
                let items = ListParser::convert_list(self, element);
                self.list_level -= 1;
                self.blocks.push(Node::OrderedList {
                    start: 1,
                    items: items?,
                });
                Ok(())
            }

            tag::ul => {
                self.flush_inline_buffer();
                self.list_level += 1;
                let items = ListParser::convert_list(self, element);
                self.list_level -= 1;
                self.blocks.push(Node::UnorderedList(items?));
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

            md_tag::figure => InlineParser::convert_figure(self, element),

            tag::p | tag::span => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::strong | md_tag::strong => InlineParser::convert_strong(self, element),

            tag::em | md_tag::emph => InlineParser::convert_emphasis(self, element),

            md_tag::highlight => InlineParser::convert_highlight(self, element),

            md_tag::strike => InlineParser::convert_strikethrough(self, element),

            md_tag::link => InlineParser::convert_link(self, element),

            md_tag::image => InlineParser::convert_image(self, element),

            md_tag::linebreak => {
                self.inline_buffer.push(Node::HardBreak);
                Ok(())
            }

            md_tag::table | md_tag::grid => {
                self.flush_inline_buffer();
                if let Some(table) = TableParser::convert_table(self, element)? {
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
    pub(crate) fn create_html_element(&mut self, element: &HtmlElement) -> Result<Node> {
        let attributes = element
            .attrs
            .0
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
                    self.inline_buffer
                        .push(MediaParser::convert_frame(self, frame));
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

    pub(crate) fn begin_list(&self, item_content: &mut Vec<Node>) {
        if self.feat.annotate_elem {
            item_content.push(Node::Custom(Box::new(Comment(format!(
                "typlite:begin:list-item {}",
                self.list_level - 1
            )))))
        }
    }

    pub(crate) fn end_list(&self, item_content: &mut Vec<Node>) {
        if self.feat.annotate_elem {
            item_content.push(Node::Custom(Box::new(Comment(format!(
                "typlite:end:list-item {}",
                self.list_level - 1
            )))))
        }
    }
}

#[derive(Debug, Clone)]
struct Comment(String);

impl CustomNode for Comment {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn write(
        &self,
        writer: &mut dyn cmark_writer::CustomNodeWriter,
    ) -> cmark_writer::WriteResult<()> {
        writer.write_str("<!-- ")?;
        writer.write_str(&self.0)?;
        writer.write_str(" -->")?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn CustomNode> {
        Box::new(self.clone())
    }

    fn eq_box(&self, other: &dyn CustomNode) -> bool {
        if let Some(other) = other.as_any().downcast_ref::<Comment>() {
            self.0 == other.0
        } else {
            false
        }
    }

    fn is_block(&self) -> bool {
        false
    }
}

impl HtmlToAstParser {
    pub fn parse(mut self, root: &HtmlElement) -> Result<Node> {
        self.blocks.clear();
        self.inline_buffer.clear();

        self.convert_element(root)?;
        self.flush_inline_buffer();

        Ok(Node::Document(self.blocks))
    }
}
