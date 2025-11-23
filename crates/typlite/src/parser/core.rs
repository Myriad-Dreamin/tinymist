//! HTML parser core, containing main structures and general parsing logic

use std::sync::Arc;

use typst::diag::SourceDiagnostic;
use typst_syntax::Span;

use cmark_writer::WriteResult;
use cmark_writer::ast::{CustomNode, HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use cmark_writer::writer::InlineWriterProxy;
use ecow::EcoString;
use tinymist_project::LspWorld;
use typst_html::{HtmlElement, HtmlNode, HtmlTag, tag};

use crate::Result;
use crate::TypliteFeat;
use crate::attributes::{
    AlertsAttr, EnumAttr, EquationAttr, HeadingAttr, ListAttr, RawAttr, TermsAttr, TypliteAttrsParser, md_attr,
};
use crate::common::{AlertNode, CenterNode, VerbatimNode};
use crate::diagnostics::WarningCollector;
use crate::tags::md_tag;

use super::{list::ListParser, table::TableParser};

/// HTML to AST parser implementation
pub struct HtmlToAstParser {
    pub asset_counter: usize,
    pub feat: TypliteFeat,
    pub world: Arc<LspWorld>,
    pub list_level: usize,
    pub blocks: Vec<Node>,
    pub inline_buffer: Vec<Node>,
    pub element_stack: Vec<HtmlTag>,
    pub(crate) warnings: WarningCollector,
}

impl HtmlToAstParser {
    pub(crate) fn new(
        feat: TypliteFeat,
        world: &Arc<LspWorld>,
        warnings: WarningCollector,
    ) -> Self {
        Self {
            feat,
            world: world.clone(),
            asset_counter: 0,
            list_level: 0,
            blocks: Vec::new(),
            inline_buffer: Vec::new(),
            element_stack: Vec::new(),
            warnings,
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
            tag::mark | md_tag::highlight => self.convert_highlight(element),
            tag::s | md_tag::strike => self.convert_strikethrough(element),

            tag::br => {
                self.inline_buffer.push(Node::HardBreak);
                Ok(())
            }

            md_tag::pagebreak => {
                self.flush_inline_buffer();
                self.blocks.push(Node::ThematicBreak);
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

            md_tag::list => {
                self.flush_inline_buffer();
                let attrs = ListAttr::parse(&element.attrs)?;
                let node = ListParser::convert_m1_list(self, element, &attrs)?;
                self.blocks.push(node);
                Ok(())
            }

            md_tag::r#enum => {
                self.flush_inline_buffer();
                let attrs = EnumAttr::parse(&element.attrs)?;
                let node = ListParser::convert_m1_enum(self, element, &attrs)?;
                self.blocks.push(node);
                Ok(())
            }

            md_tag::terms => {
                self.flush_inline_buffer();
                let attrs = TermsAttr::parse(&element.attrs)?;
                self.convert_terms(element, &attrs)?;
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

            md_tag::equation => self.convert_equation(element),

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
                    // self.warn_at(
                    //     Some(element.span),
                    //     eco_format!(
                    //         "unsupported HTML element `<{tag_name}>`; exported as raw HTML"
                    //     ),
                    // );
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

        let (inline_nodes, block_nodes) = self.capture_children(element)?;

        let mut children = Vec::new();
        if !inline_nodes.is_empty() {
            children.extend(inline_nodes);
        }
        children.extend(block_nodes);

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

    fn convert_children_impl(&mut self, element: &HtmlElement) -> Result<()> {
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => {
                    self.inline_buffer.push(Node::Text(text.clone()));
                }
                HtmlNode::Element(element) => {
                    self.convert_element(element)?;
                }
                HtmlNode::Frame(frame) => {
                    let res = self.convert_frame(&frame.inner);
                    self.inline_buffer.push(res);
                }
                HtmlNode::Tag(..) => {}
            }
        }
        Ok(())
    }

    pub fn convert_children(&mut self, element: &HtmlElement) -> Result<()> {
        self.element_stack.push(element.tag);
        let result = self.convert_children_impl(element);
        self.element_stack.pop();
        result
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

    /// Convert element children while capturing both inline and block outputs.
    pub fn capture_children(&mut self, element: &HtmlElement) -> Result<(Vec<Node>, Vec<Node>)> {
        let prev_buffer = std::mem::take(&mut self.inline_buffer);
        let prev_blocks = std::mem::take(&mut self.blocks);

        self.convert_children(element)?;

        let inline = std::mem::take(&mut self.inline_buffer);
        let blocks = std::mem::take(&mut self.blocks);

        self.inline_buffer = prev_buffer;
        self.blocks = prev_blocks;

        Ok((inline, blocks))
    }

    pub(crate) fn warn_at(&mut self, span: Option<Span>, message: EcoString) {
        let span = span.unwrap_or_else(Span::detached);
        let span = self
            .feat
            .wrap_info
            .as_ref()
            .and_then(|info| self.remap_span_from_wrapper(span, info))
            .unwrap_or(span);

        let diag = SourceDiagnostic::warning(span, message);
        self.warnings.extend(std::iter::once(diag));
    }

    fn remap_span_from_wrapper(&self, span: Span, info: &crate::WrapInfo) -> Option<Span> {
        info.remap_span(self.world.as_ref(), span)
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

    fn write_inline(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
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
    pub fn convert_terms(&mut self, element: &HtmlElement, _attrs: &TermsAttr) -> Result<()> {
        for child in &element.children {
            if let HtmlNode::Element(item) = child
                && item.tag == md_tag::item
            {
                let mut term_nodes = Vec::new();
                let mut desc_nodes = Vec::new();

                for part in &item.children {
                    if let HtmlNode::Element(part_elem) = part {
                        if part_elem.tag == md_tag::term_entry {
                            self.convert_children_into(&mut term_nodes, part_elem)?;
                        } else if part_elem.tag.resolve().as_str() == "m1description" {
                            self.convert_children_into(&mut desc_nodes, part_elem)?;
                        }
                    }
                }

                if term_nodes.is_empty() && desc_nodes.is_empty() {
                    continue;
                }

                let mut paragraph = Vec::new();
                if !term_nodes.is_empty() {
                    paragraph.push(Node::Strong(term_nodes));
                    paragraph.push(Node::Text(EcoString::from(": ")));
                }
                paragraph.extend(desc_nodes);
                self.blocks.push(Node::Paragraph(paragraph));
            }
        }

        Ok(())
    }

    pub fn convert_equation(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = EquationAttr::parse(&element.attrs)?;
        if attrs.block {
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

        if element.tag == md_tag::list {
            let attrs = ListAttr::parse(&element.attrs)?;
            let node = super::list::ListParser::convert_m1_list(self, element, &attrs)?;
            return Ok(vec![node]);
        }

        if element.tag == md_tag::r#enum {
            let attrs = EnumAttr::parse(&element.attrs)?;
            let node = super::list::ListParser::convert_m1_enum(self, element, &attrs)?;
            return Ok(vec![node]);
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
