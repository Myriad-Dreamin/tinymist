//! Inline element processing module, handles text and inline style elements

use cmark_writer::ast::Node;
use typst::html::HtmlElement;

use crate::Result;
use crate::attributes::{FigureAttr, ImageAttr, LinkAttr, TypliteAttrsParser};
use crate::common::{CenterNode, FigureNode, HighlightNode};

use super::core::HtmlToAstParser;

impl HtmlToAstParser {
    /// Convert strong emphasis element
    pub fn convert_strong(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Node::Strong(content));
        Ok(())
    }

    /// Convert emphasis element
    pub fn convert_emphasis(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Node::Emphasis(content));
        Ok(())
    }

    /// Convert highlight element
    pub fn convert_highlight(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer
            .push(Node::Custom(Box::new(HighlightNode { content })));
        Ok(())
    }

    /// Convert strikethrough element
    pub fn convert_strikethrough(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Node::Strikethrough(content));
        Ok(())
    }

    /// Convert link element
    pub fn convert_link(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&element.attrs)?;
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Node::Link {
            url: attrs.dest,
            title: None,
            content,
        });
        Ok(())
    }

    /// Convert image element
    pub fn convert_image(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        self.inline_buffer.push(Node::Image {
            url: attrs.src,
            title: None,
            alt: vec![Node::Text(attrs.alt)],
        });
        Ok(())
    }

    /// Convert figure element
    pub fn convert_figure(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        // Parse figure attributes to extract caption
        let attrs = FigureAttr::parse(&element.attrs)?;
        let caption = attrs.caption.to_string();

        // Find image and body content
        let mut body_content = Vec::new();
        self.convert_children_into(&mut body_content, element)?;
        let body = Box::new(Node::Paragraph(body_content));

        // Create figure node with centering
        let figure_node = Box::new(FigureNode { body, caption });
        let centered_node = CenterNode::new(vec![Node::Custom(figure_node)]);

        // Add the centered figure to blocks
        self.blocks.push(Node::Custom(Box::new(centered_node)));

        Ok(())
    }
}
