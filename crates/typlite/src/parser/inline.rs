//! Inline element processing module, handles text and inline style elements.

use typst_html::{HtmlElement, HtmlNode};

use crate::Result;
use crate::attributes::{FigureAttr, ImageAttr, LinkAttr, TypliteAttrsParser, md_attr};
use crate::ir::{Block, Inline};
use crate::tags::md_tag;

use super::core::HtmlToIrParser;

impl HtmlToIrParser {
    /// Convert strong emphasis element.
    pub fn convert_strong(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Inline::Strong(content));
        Ok(())
    }

    /// Convert emphasis element.
    pub fn convert_emphasis(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Inline::Emphasis(content));
        Ok(())
    }

    /// Convert highlight element.
    pub fn convert_highlight(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Inline::Highlight(content));
        Ok(())
    }

    /// Convert strikethrough element.
    pub fn convert_strikethrough(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(Inline::Strikethrough(content));
        Ok(())
    }

    /// Convert link element.
    pub fn convert_link(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&element.attrs)?;
        let mut url = attrs.dest;
        let mut content = Vec::new();

        for child in &element.children {
            match child {
                HtmlNode::Element(child_elem) => {
                    if child_elem.tag == md_tag::link_body || child_elem.tag == md_tag::body {
                        self.convert_children_into(&mut content, child_elem)?;
                    } else if child_elem.tag == md_tag::link_dest {
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
                HtmlNode::Text(text, _) => content.push(Inline::Text(text.clone())),
                HtmlNode::Frame(frame) => {
                    content.push(self.convert_frame(&frame.inner));
                }
                HtmlNode::Tag(..) => {}
            }
        }

        if content.is_empty() {
            self.convert_children_into(&mut content, element)?;
        }

        self.inline_buffer.push(Inline::Link {
            url,
            title: None,
            content,
        });
        Ok(())
    }

    /// Convert image element.
    pub fn convert_image(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        self.inline_buffer.push(Inline::Image {
            url: attrs.source,
            title: None,
            alt: vec![Inline::Text(attrs.alt)],
        });
        Ok(())
    }

    /// Convert figure element.
    pub fn convert_figure(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        let _attrs = FigureAttr::parse(&element.attrs)?;
        let mut caption_nodes = Vec::new();
        let mut inline_segments: Vec<Vec<Inline>> = Vec::new();
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

        let mut content_nodes = Vec::new();
        if !inline_segments.is_empty() {
            for segment in inline_segments {
                content_nodes.push(Block::Paragraph(segment));
            }
        }
        content_nodes.append(&mut block_content);

        let body = if content_nodes.is_empty() {
            Box::new(Block::Paragraph(Vec::new()))
        } else if content_nodes.len() == 1 {
            Box::new(content_nodes.into_iter().next().unwrap())
        } else {
            Box::new(Block::Document(content_nodes))
        };

        let figure_block = Block::Figure {
            body,
            caption: caption_nodes,
        };

        self.blocks.push(Block::Center(Box::new(figure_block)));

        Ok(())
    }
}
