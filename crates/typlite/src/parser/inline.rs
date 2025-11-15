//! Inline element processing module, handles text and inline style elements

use cmark_writer::ast::Node;
use typst_html::{HtmlElement, HtmlNode, tag};

use crate::Result;
use crate::attributes::{FigureAttr, TypliteAttrsParser};
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
        let dest = self.attr_value(element, "href").unwrap_or_default();
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Node::Link {
            url: dest,
            title: None,
            content,
        });
        Ok(())
    }

    /// Convert image element
    pub fn convert_image(&mut self, element: &HtmlElement) -> Result<()> {
        let src = self.attr_value(element, "src").unwrap_or_default();
        let alt = self.attr_value(element, "alt").unwrap_or_default();

        self.inline_buffer.push(Node::Image {
            url: src,
            title: None,
            alt: vec![Node::Text(alt)],
        });
        Ok(())
    }

    /// Convert figure element
    pub fn convert_figure(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        // Parse figure attributes to extract caption
        let attrs = FigureAttr::parse(&element.attrs)?;
        let mut caption = attrs.caption.to_string();

        let prev_blocks = std::mem::take(&mut self.blocks);
        let prev_buffer = std::mem::take(&mut self.inline_buffer);

        for child in &element.children {
            if let HtmlNode::Element(child_elem) = child
                && child_elem.tag == tag::figcaption
            {
                if caption.is_empty() {
                    caption = self.extract_plain_text(child_elem).to_string();
                }
                continue;
            }

            self.convert_child(child)?;
        }

        let inline_content = std::mem::take(&mut self.inline_buffer);
        let mut block_content = std::mem::take(&mut self.blocks);

        self.inline_buffer = prev_buffer;
        self.blocks = prev_blocks;

        let mut content_nodes = Vec::new();
        if !inline_content.is_empty() {
            content_nodes.push(Node::Paragraph(inline_content));
        }
        content_nodes.append(&mut block_content);

        let body = if content_nodes.is_empty() {
            Box::new(Node::Paragraph(Vec::new()))
        } else if content_nodes.len() == 1 {
            Box::new(content_nodes.into_iter().next().unwrap())
        } else {
            Box::new(Node::Document(content_nodes))
        };

        // Create figure node with centering
        let figure_node = Box::new(FigureNode { body, caption });
        let centered_node = CenterNode::new(vec![Node::Custom(figure_node)]);

        // Add the centered figure to blocks
        self.blocks.push(Node::Custom(Box::new(centered_node)));

        Ok(())
    }
}
