//! Inline element processing module, handles text and inline style elements

use cmark_writer::ast::Node;
use typst_html::{HtmlElement, HtmlNode};

use crate::Result;
use crate::attributes::{FigureAttr, ImageAttr, LinkAttr, TypliteAttrsParser, md_attr};
use crate::common::{CenterNode, FigureNode, HighlightNode};
use crate::tags::md_tag;

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
        let mut url = attrs.dest;
        let mut content = Vec::new();

        for child in &element.children {
            match child {
                HtmlNode::Element(child_elem) => {
                    if child_elem.tag == md_tag::link_body || child_elem.tag == md_tag::body {
                        self.convert_children_into(&mut content, child_elem)?;
                    } else if child_elem.tag == md_tag::link_dest
                    {
                        if let Some((_, value)) = child_elem
                            .attrs
                            .0
                            .iter()
                            .find(|(name, _)| *name == md_attr::dest)
                        {
                            url = value.clone();
                        }
                    } else {
                        let mut extra = Vec::new();
                        self.convert_children_into(&mut extra, child_elem)?;
                        content.extend(extra);
                    }
                }
                HtmlNode::Text(text, _) => content.push(Node::Text(text.clone())),
                HtmlNode::Frame(frame) => {
                    content.push(self.convert_frame(&frame.inner));
                }
                HtmlNode::Tag(..) => {}
            }
        }

        if content.is_empty() {
            self.convert_children_into(&mut content, element)?;
        }

        self.inline_buffer.push(Node::Link {
            url,
            title: None,
            content,
        });
        Ok(())
    }

    /// Convert image element
    pub fn convert_image(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        self.inline_buffer.push(Node::Image {
            url: attrs.source,
            title: None,
            alt: vec![Node::Text(attrs.alt)],
        });
        Ok(())
    }

    /// Convert figure element
    pub fn convert_figure(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        // Parse figure attributes to extract caption
        let _attrs = FigureAttr::parse(&element.attrs)?;
        let mut caption_nodes = Vec::new();
        let mut inline_segments: Vec<Vec<Node>> = Vec::new();
        let mut block_content = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(child_elem) = child {
                if child_elem.tag == md_tag::figure_body || child_elem.tag == md_tag::body {
                    let (inline, mut blocks) = self.capture_children(child_elem)?;
                    if !inline.is_empty() {
                        inline_segments.push(inline);
                    }
                    block_content.append(&mut blocks);
                } else if child_elem.tag == md_tag::figure_caption {
                    self.convert_children_into(&mut caption_nodes, child_elem)?;
                }
            }
        }

        if inline_segments.is_empty() && block_content.is_empty() {
            let (inline, mut blocks) = self.capture_children(element)?;
            if !inline.is_empty() {
                inline_segments.push(inline);
            }
            block_content.append(&mut blocks);
        }

        let caption = caption_nodes
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        let mut content_nodes = Vec::new();
        if !inline_segments.is_empty() {
            for segment in inline_segments {
                content_nodes.push(Node::Paragraph(segment));
            }
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
