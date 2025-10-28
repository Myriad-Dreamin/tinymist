//! Inline element processing module, handles text and inline style elements

use base64::Engine;
use cmark_writer::ast::Node;
use ecow::eco_format;
use typst::html::{HtmlElement, HtmlNode};

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

    /// Convert image element with bytes source
    pub fn convert_image_bytes(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;

        let Some(HtmlNode::Frame(frame)) = element.children.first() else {
            // should not happen
            log::warn!("Image with bytes source has no frame in children");
            self.inline_buffer.push(Node::Image {
                url: eco_format!(""),
                title: None,
                alt: vec![Node::Text(attrs.alt)],
            });
            return Ok(());
        };
        let svg = typst_svg::svg_frame(frame);

        let url = if let Some(assets_path) = &self.feat.assets_path {
            let file_id = self.asset_counter;
            self.asset_counter += 1;
            let file_name = format!("image_{file_id}.svg");
            let file_path = assets_path.join(&file_name);

            std::fs::write(&file_path, svg.as_bytes())?;

            eco_format!("{file_name}")
        } else {
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&svg);
            eco_format!("data:image/svg+xml;base64,{base64_data}")
        };

        self.inline_buffer.push(Node::Image {
            url,
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

        let (inline_content, mut block_content) = self.capture_children(element)?;

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
