//! Inline element processing module, handles text and inline style elements

use cmark_writer::ast::Node;
use typst::html::HtmlElement;

use crate::attributes::{FigureAttr, ImageAttr, LinkAttr, TypliteAttrsParser};
use crate::common::{FigureNode, HighlightNode};
use crate::Result;

use super::core::HtmlToAstParser;

/// Inline style element parser
pub struct InlineParser;

impl InlineParser {
    /// Convert strong emphasis element
    pub fn convert_strong(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        parser.convert_children_into(&mut content, element)?;
        parser.inline_buffer.push(Node::Strong(content));
        Ok(())
    }

    /// Convert emphasis element
    pub fn convert_emphasis(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        parser.convert_children_into(&mut content, element)?;
        parser.inline_buffer.push(Node::Emphasis(content));
        Ok(())
    }

    /// Convert highlight element
    pub fn convert_highlight(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        parser.convert_children_into(&mut content, element)?;
        parser
            .inline_buffer
            .push(Node::Custom(Box::new(HighlightNode { content })));
        Ok(())
    }

    /// Convert strikethrough element
    pub fn convert_strikethrough(
        parser: &mut HtmlToAstParser,
        element: &HtmlElement,
    ) -> Result<()> {
        let mut content = Vec::new();
        parser.convert_children_into(&mut content, element)?;
        parser.inline_buffer.push(Node::Strikethrough(content));
        Ok(())
    }

    /// Convert link element
    pub fn convert_link(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&element.attrs)?;
        let mut content = Vec::new();
        parser.convert_children_into(&mut content, element)?;
        parser.inline_buffer.push(Node::Link {
            url: attrs.dest.into(),
            title: None,
            content,
        });
        Ok(())
    }

    /// Convert image element
    pub fn convert_image(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        let src = attrs.src.as_str();
        parser.inline_buffer.push(Node::Image {
            url: src.to_string(),
            title: None,
            alt: vec![Node::Text(attrs.alt.into())],
        });
        Ok(())
    }

    /// Convert figure element
    pub fn convert_figure(parser: &mut HtmlToAstParser, element: &HtmlElement) -> Result<()> {
        parser.flush_inline_buffer();

        // Parse figure attributes to extract caption
        let attrs = FigureAttr::parse(&element.attrs)?;
        let caption = attrs.caption.to_string();

        // Find image and body content
        let mut body_content = Vec::new();
        parser.convert_children_into(&mut body_content, element)?;
        let body = Box::new(Node::Paragraph(body_content));

        // Create figure node using generic definition
        parser
            .blocks
            .push(Node::Custom(Box::new(FigureNode { body, caption })));

        Ok(())
    }
}
