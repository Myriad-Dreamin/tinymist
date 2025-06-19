//! HTML parser core, containing main structures and general parsing logic

use std::sync::Arc;

use cmark_writer::ast::{CustomNode, HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use cmark_writer::{CommonMarkWriter, WriteResult};
use ecow::EcoString;
use tinymist_project::LspWorld;
use typst::html::{tag, HtmlElement, HtmlNode};

use crate::attributes::{md_attr, AlertsAttr, HeadingAttr, RawAttr, TypliteAttrsParser};
use crate::common::{AlertNode, CenterNode, VerbatimNode};
use crate::tags::md_tag;
use crate::Result;
use crate::TypliteFeat;

use super::{list::ListParser, table::TableParser};

/// HTML to AST parser implementation
pub struct HtmlToAstParser {
    pub asset_counter: usize,
    pub feat: TypliteFeat,
    pub world: Arc<LspWorld>,
    pub list_level: usize,
    pub blocks: Vec<Node>,
    pub inline_buffer: Vec<Node>,
}

impl HtmlToAstParser {
    pub fn new(feat: TypliteFeat, world: &Arc<LspWorld>) -> Self {
        Self {
            feat,
            world: world.clone(),
            asset_counter: 0,
            list_level: 0,
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

            tag::p | tag::span | tag::div => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::strong | md_tag::strong => self.convert_strong(element),
            tag::em | md_tag::emph => self.convert_emphasis(element),

            tag::br => {
                self.inline_buffer.push(Node::HardBreak);
                Ok(())
            }

            tag::ol => {
                self.flush_inline_buffer();
                let items = ListParser::convert_list(self, element);
                self.blocks.push(Node::OrderedList {
                    start: 1,
                    items: items?,
                });
                Ok(())
            }

            tag::ul => {
                self.flush_inline_buffer();
                let items = ListParser::convert_list(self, element);
                self.blocks.push(Node::UnorderedList(items?));
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

            md_tag::raw => {
                let attrs = RawAttr::parse(&element.attrs)?;
                if attrs.block {
                    self.flush_inline_buffer();
                    self.blocks
                        .push(Node::code_block(Some(attrs.lang), attrs.text));
                } else {
                    self.inline_buffer.push(Node::InlineCode(attrs.text));
                }
                Ok(())
            }

            md_tag::quote => {
                let prev_blocks = std::mem::take(&mut self.blocks);
                self.flush_inline_buffer();
                self.convert_children(element)?;
                let content = Node::Paragraph(std::mem::take(&mut self.inline_buffer));
                let mut quote = std::mem::take(&mut self.blocks);
                quote.push(content);
                self.blocks.clear();
                self.blocks.extend(prev_blocks);
                self.blocks.push(Node::BlockQuote(quote));
                Ok(())
            }

            md_tag::figure => self.convert_figure(element),
            md_tag::highlight => self.convert_highlight(element),
            md_tag::strike => self.convert_strikethrough(element),
            md_tag::link => self.convert_link(element),
            md_tag::image => self.convert_image(element),

            md_tag::linebreak => {
                self.inline_buffer.push(Node::HardBreak);
                Ok(())
            }

            md_tag::source => {
                let src = self.convert_source(element);
                self.inline_buffer.push(src);
                Ok(())
            }

            md_tag::table | md_tag::grid => {
                self.flush_inline_buffer();
                if let Some(table) = TableParser::convert_table(self, element)? {
                    self.blocks.push(table);
                }
                Ok(())
            }

            md_tag::idoc => {
                let src = self.convert_idoc(element);
                self.inline_buffer.push(src);
                Ok(())
            }

            md_tag::math_equation_inline | md_tag::math_equation_block => {
                if element.tag == md_tag::math_equation_block {
                    self.flush_inline_buffer();
                    self.convert_children(element)?;
                    let content = std::mem::take(&mut self.inline_buffer);
                    self.blocks
                        .push(Node::Custom(Box::new(CenterNode::new(content))));
                } else {
                    self.convert_children(element)?;
                }
                Ok(())
            }

            md_tag::alerts => {
                self.flush_inline_buffer();
                let attrs = AlertsAttr::parse(&element.attrs)?;
                let prev_blocks = std::mem::take(&mut self.blocks);
                self.flush_inline_buffer();
                self.convert_children(element)?;
                let content = Node::Paragraph(std::mem::take(&mut self.inline_buffer));
                let mut quote = std::mem::take(&mut self.blocks);
                quote.push(content);
                self.blocks.clear();
                self.blocks.extend(prev_blocks);
                self.blocks.push(Node::Custom(Box::new(AlertNode {
                    content: quote,
                    class: attrs.class,
                })));
                Ok(())
            }

            md_tag::verbatim => {
                self.inline_buffer.push(Node::Custom(Box::new(VerbatimNode {
                    content: element
                        .attrs
                        .0
                        .iter()
                        .find(|(name, _)| *name == md_attr::src)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default(),
                })));
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
                name: name.resolve().to_string().into(),
                value: value.clone(),
            })
            .collect();

        let mut children = Vec::new();
        self.convert_children_into(&mut children, element)?;

        Ok(Node::HtmlElement(CmarkHtmlElement {
            tag: element.tag.resolve().to_string().into(),
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
                    self.inline_buffer.push(Node::Text(text.clone()));
                }
                HtmlNode::Element(element) => {
                    self.convert_element(element)?;
                }
                HtmlNode::Frame(frame) => {
                    let res = self.convert_frame(frame);
                    self.inline_buffer.push(res);
                }
                HtmlNode::Tag(..) => {}
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
}

#[derive(Debug, Clone)]
pub(crate) struct Comment(pub EcoString);

impl CustomNode for Comment {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn write(&self, writer: &mut CommonMarkWriter) -> WriteResult<()> {
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
    pub fn is_block_element(element: &HtmlElement) -> bool {
        matches!(
            element.tag,
            tag::p
                | tag::div
                | tag::blockquote
                | tag::h1
                | tag::h2
                | tag::h3
                | tag::h4
                | tag::h5
                | tag::h6
                | tag::hr
                | tag::pre
                | tag::table
                | tag::section
                | tag::article
                | tag::header
                | tag::footer
                | tag::main
                | tag::aside
                | tag::nav
                | tag::ul
                | tag::ol
                | md_tag::heading
                | md_tag::quote
                | md_tag::raw
                | md_tag::parbreak
                | md_tag::table
                | md_tag::grid
                | md_tag::figure
        )
    }

    pub fn process_list_item_element(&mut self, element: &HtmlElement) -> Result<Vec<Node>> {
        if element.tag == tag::ul || element.tag == tag::ol {
            let items = super::list::ListParser::convert_list(self, element)?;
            if element.tag == tag::ul {
                return Ok(vec![Node::UnorderedList(items)]);
            } else {
                return Ok(vec![Node::OrderedList { start: 1, items }]);
            }
        }

        let prev_blocks = std::mem::take(&mut self.blocks);
        let prev_buffer = std::mem::take(&mut self.inline_buffer);

        self.convert_element(element)?;
        let mut result = Vec::new();

        if !self.blocks.is_empty() {
            result.extend(std::mem::take(&mut self.blocks));
        } else if !self.inline_buffer.is_empty() {
            if Self::is_block_element(element) {
                result.push(Node::Paragraph(std::mem::take(&mut self.inline_buffer)));
            } else {
                result = std::mem::take(&mut self.inline_buffer);
            }
        }

        self.blocks = prev_blocks;
        self.inline_buffer = prev_buffer;

        Ok(result)
    }

    pub fn parse(mut self, root: &HtmlElement) -> Result<Node> {
        self.blocks.clear();
        self.inline_buffer.clear();

        self.convert_element(root)?;
        self.flush_inline_buffer();

        Ok(Node::Document(self.blocks))
    }
}
