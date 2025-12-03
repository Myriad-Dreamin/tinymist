//! Utility functions for HTML writing.

use crate::ast::{HeadingType, HtmlElement, ListItem, Node, TableAlignment, TableRow};
use crate::writer::runtime::visitor::{walk_node, NodeHandler};
use ecow::EcoString;
use std::convert::Infallible;

/// Check if an HTML tag name is safe.
///
/// Tag names should only contain letters, numbers, underscores, colons, and hyphens.
pub(crate) fn is_safe_tag_name(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}

/// Check if an HTML attribute name is safe.
///
/// Attribute names should only contain letters, numbers, underscores, colons, dots, and hyphens.
pub(crate) fn is_safe_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-' || c == '.')
}

struct PlainTextCollector<'a> {
    buffer: &'a mut EcoString,
}

impl<'a> PlainTextCollector<'a> {
    fn new(buffer: &'a mut EcoString) -> Self {
        Self { buffer }
    }

    fn push_space(&mut self) {
        if !self.buffer.is_empty() && !self.buffer.ends_with(' ') {
            self.buffer.push(' ');
        }
    }

    fn collect_list_items(&mut self, items: &[ListItem]) -> Result<(), Infallible> {
        for item in items {
            match item {
                ListItem::Unordered { content } | ListItem::Ordered { content, .. } => {
                    NodeHandler::visit_nodes(self, content)?;
                }
                #[cfg(feature = "gfm")]
                ListItem::Task { content, .. } => {
                    NodeHandler::visit_nodes(self, content)?;
                }
            }
        }
        Ok(())
    }
}

impl NodeHandler for PlainTextCollector<'_> {
    type Error = Infallible;

    fn document(&mut self, children: &[Node]) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, children)
    }

    fn paragraph(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, content)?;
        self.push_space();
        Ok(())
    }

    fn heading(
        &mut self,
        _level: u8,
        content: &[Node],
        _heading_type: &HeadingType,
    ) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, content)?;
        self.push_space();
        Ok(())
    }

    fn block_quote(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, content)?;
        self.push_space();
        Ok(())
    }

    fn text(&mut self, text: &EcoString) -> Result<(), Self::Error> {
        self.buffer.push_str(text);
        Ok(())
    }

    fn inline_code(&mut self, code: &EcoString) -> Result<(), Self::Error> {
        self.buffer.push_str(code);
        Ok(())
    }

    fn html_element(&mut self, element: &HtmlElement) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, &element.children)
    }

    fn unordered_list(&mut self, items: &[ListItem]) -> Result<(), Self::Error> {
        self.collect_list_items(items)
    }

    fn ordered_list(&mut self, _start: u32, items: &[ListItem]) -> Result<(), Self::Error> {
        self.collect_list_items(items)
    }

    fn image(
        &mut self,
        _url: &EcoString,
        _title: &Option<EcoString>,
        alt: &[Node],
    ) -> Result<(), Self::Error> {
        NodeHandler::visit_nodes(self, alt)
    }

    fn soft_break(&mut self) -> Result<(), Self::Error> {
        self.push_space();
        Ok(())
    }

    fn hard_break(&mut self) -> Result<(), Self::Error> {
        self.push_space();
        Ok(())
    }

    fn autolink(&mut self, url: &EcoString, _is_email: bool) -> Result<(), Self::Error> {
        self.buffer.push_str(url);
        Ok(())
    }

    #[cfg(feature = "gfm")]
    fn extended_autolink(&mut self, url: &EcoString) -> Result<(), Self::Error> {
        self.buffer.push_str(url);
        Ok(())
    }

    fn table(
        &mut self,
        _columns: usize,
        rows: &[TableRow],
        _alignments: &[TableAlignment],
    ) -> Result<(), Self::Error> {
        for row in rows {
            for cell in &row.cells {
                NodeHandler::visit_node(self, &cell.content)?;
            }
        }
        Ok(())
    }
}

/// Render a list of nodes into plain text, used for alt text and diagnostics.
pub(crate) fn render_nodes_to_plain_text(nodes: &[Node], buffer: &mut EcoString) {
    let mut collector = PlainTextCollector::new(buffer);
    for node in nodes {
        let _ = walk_node(&mut collector, node);
    }
}

/// Render nodes to a plain-text string helper.
pub(crate) fn render_nodes_to_plain_text_string(nodes: &[Node]) -> EcoString {
    let mut s = EcoString::new();
    render_nodes_to_plain_text(nodes, &mut s);
    s
}
