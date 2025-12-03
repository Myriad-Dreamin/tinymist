//! Shared AST visitor utilities used by writer backends.
//!
//! The goal of this module is to provide a single dispatch table for the
//! `Node` enum so that different backends (CommonMark, HTML, etc.) can
//! implement `NodeHandler` and reuse the traversal logic without
//! copy-pasting large `match` expressions.

use crate::ast::{
    CodeBlockType, CustomNode, HeadingType, HtmlElement, ListItem, Node, TableAlignment, TableRow,
};
use ecow::EcoString;

/// Trait implemented by writer backends that want to consume the AST.
#[allow(missing_docs)]
pub trait NodeHandler {
    /// Error type produced during traversal.
    type Error;

    /// Dispatch a single node. Most implementers will not override this and
    /// will instead implement the per-variant methods below.
    fn visit_node(&mut self, node: &Node) -> Result<(), Self::Error> {
        walk_node(self, node)
    }

    /// Visit a sequence of nodes.
    fn visit_nodes(&mut self, nodes: &[Node]) -> Result<(), Self::Error> {
        for node in nodes {
            self.visit_node(node)?;
        }
        Ok(())
    }

    fn document(&mut self, children: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(children)
    }

    fn paragraph(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn text(&mut self, _text: &EcoString) -> Result<(), Self::Error> {
        Ok(())
    }

    fn emphasis(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn strong(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn thematic_break(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn heading(
        &mut self,
        _level: u8,
        content: &[Node],
        _heading_type: &HeadingType,
    ) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn inline_code(&mut self, _code: &EcoString) -> Result<(), Self::Error> {
        Ok(())
    }

    fn code_block(
        &mut self,
        _language: &Option<EcoString>,
        _content: &EcoString,
        _kind: &CodeBlockType,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn html_block(&mut self, _content: &EcoString) -> Result<(), Self::Error> {
        Ok(())
    }

    fn html_element(&mut self, _element: &HtmlElement) -> Result<(), Self::Error> {
        Ok(())
    }

    fn block_quote(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn unordered_list(&mut self, _items: &[ListItem]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn ordered_list(&mut self, _start: u32, _items: &[ListItem]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn table(
        &mut self,
        _columns: usize,
        _rows: &[TableRow],
        _alignments: &[TableAlignment],
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn link(
        &mut self,
        _url: &EcoString,
        _title: &Option<EcoString>,
        content: &[Node],
    ) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn image(
        &mut self,
        _url: &EcoString,
        _title: &Option<EcoString>,
        _alt: &[Node],
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn soft_break(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn hard_break(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn autolink(&mut self, _url: &EcoString, _is_email: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    #[cfg(feature = "gfm")]
    fn extended_autolink(&mut self, _url: &EcoString) -> Result<(), Self::Error> {
        Ok(())
    }

    fn link_reference_definition(
        &mut self,
        _label: &EcoString,
        _destination: &EcoString,
        _title: &Option<EcoString>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reference_link(&mut self, _label: &EcoString, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    #[cfg(feature = "gfm")]
    fn strikethrough(&mut self, content: &[Node]) -> Result<(), Self::Error> {
        self.visit_nodes(content)
    }

    fn custom(&mut self, _node: &dyn CustomNode) -> Result<(), Self::Error> {
        Ok(())
    }

    fn unsupported(&mut self, _node: &Node) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Dispatch a single node to the provided handler.
pub fn walk_node<H: NodeHandler + ?Sized>(handler: &mut H, node: &Node) -> Result<(), H::Error> {
    match node {
        Node::Document(children) => handler.document(children),
        Node::Paragraph(content) => handler.paragraph(content),
        Node::Text(text) => handler.text(text),
        Node::Emphasis(content) => handler.emphasis(content),
        Node::Strong(content) => handler.strong(content),
        Node::ThematicBreak => handler.thematic_break(),
        Node::Heading {
            level,
            content,
            heading_type,
        } => handler.heading(*level, content, heading_type),
        Node::InlineCode(code) => handler.inline_code(code),
        Node::CodeBlock {
            language,
            content,
            block_type,
        } => handler.code_block(language, content, block_type),
        Node::HtmlBlock(content) => handler.html_block(content),
        Node::HtmlElement(element) => handler.html_element(element),
        Node::BlockQuote(content) => handler.block_quote(content),
        Node::UnorderedList(items) => handler.unordered_list(items),
        Node::OrderedList { start, items } => handler.ordered_list(*start, items),
        Node::Link {
            url,
            title,
            content,
        } => handler.link(url, title, content),
        Node::Image { url, title, alt } => handler.image(url, title, alt),
        Node::Autolink { url, is_email } => handler.autolink(url, *is_email),
        Node::SoftBreak => handler.soft_break(),
        Node::HardBreak => handler.hard_break(),
        Node::LinkReferenceDefinition {
            label,
            destination,
            title,
        } => handler.link_reference_definition(label, destination, title),
        Node::ReferenceLink { label, content } => handler.reference_link(label, content),
        Node::Custom(custom_node) => handler.custom(custom_node.as_ref()),
        Node::Table {
            columns,
            rows,
            alignments,
        } => handler.table(*columns, rows, alignments),
        #[cfg(feature = "gfm")]
        Node::Strikethrough(content) => handler.strikethrough(content),
        #[cfg(feature = "gfm")]
        Node::ExtendedAutolink(url) => handler.extended_autolink(url),
        #[allow(unreachable_patterns)]
        _ => handler.unsupported(node),
    }
}
