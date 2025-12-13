//! Text writer implementation - produces plain text output

use ecow::EcoString;

use crate::Result;
use crate::ir::{self, Block, Inline, IrNode, ListItem};
use crate::writer::IrFormatWriter;

/// Text writer implementation
#[derive(Default)]
pub struct TextWriter {}

impl TextWriter {
    pub fn new() -> Self {
        Self {}
    }

    fn write_document(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        for block in &document.blocks {
            self.write_block(block, output)?;
            output.push_str("\n");
        }
        Ok(())
    }

    fn write_ir_node(&mut self, node: &IrNode, output: &mut EcoString) -> Result<()> {
        match node {
            IrNode::Block(block) => self.write_block(block, output),
            IrNode::Inline(inline) => self.write_inline(inline, output),
        }
    }

    fn write_inline_nodes(&mut self, nodes: &[Inline], output: &mut EcoString) -> Result<()> {
        for inline in nodes {
            self.write_inline(inline, output)?;
        }
        Ok(())
    }

    fn write_block(&mut self, node: &Block, output: &mut EcoString) -> Result<()> {
        match node {
            Block::Document(blocks) => {
                for block in blocks {
                    self.write_block(block, output)?;
                    output.push_str("\n");
                }
            }
            Block::Paragraph(inlines) => {
                self.write_inline_nodes(inlines, output)?;
                output.push_str("\n");
            }
            Block::Heading { content, .. } => {
                self.write_inline_nodes(content, output)?;
                output.push_str("\n");
            }
            Block::BlockQuote(content) => {
                for block in content {
                    self.write_block(block, output)?;
                }
            }
            Block::CodeBlock { content, .. } => {
                output.push_str(content);
                output.push_str("\n\n");
            }
            Block::OrderedList { items, .. } | Block::UnorderedList(items) => {
                for item in items {
                    match item {
                        ListItem::Ordered { content, .. } | ListItem::Unordered { content } => {
                            for block in content {
                                self.write_block(block, output)?;
                            }
                        }
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        for node in &cell.content {
                            self.write_ir_node(node, output)?;
                        }
                        output.push(' ');
                    }
                    output.push('\n');
                }
                output.push('\n');
            }
            Block::ThematicBreak => {
                output.push_str("\n");
            }
            Block::HtmlElement(element) => {
                for child in &element.children {
                    self.write_ir_node(child, output)?;
                }
            }
            Block::Figure { body, caption } => {
                self.write_block(body, output)?;
                if !caption.is_empty() {
                    output.push_str("\n");
                    self.write_inline_nodes(caption, output)?;
                }
            }
            Block::ExternalFrame(frame) => {
                if !frame.alt_text.is_empty() {
                    output.push_str(&frame.alt_text);
                }
            }
            Block::Center(inner) => {
                self.write_block(inner, output)?;
            }
            Block::Alert { content, .. } => {
                for block in content {
                    self.write_block(block, output)?;
                }
            }
            Block::HtmlBlock(_) => {}
        }

        Ok(())
    }

    fn write_inline(&mut self, node: &Inline, output: &mut EcoString) -> Result<()> {
        match node {
            Inline::Text(text) => output.push_str(text),
            Inline::Emphasis(content)
            | Inline::Strong(content)
            | Inline::Strikethrough(content)
            | Inline::Group(content)
            | Inline::Highlight(content) => {
                self.write_inline_nodes(content, output)?;
            }
            Inline::Link { content, .. } => {
                self.write_inline_nodes(content, output)?;
            }
            Inline::ReferenceLink { label, content } => {
                if content.is_empty() {
                    output.push_str(label);
                } else {
                    self.write_inline_nodes(content, output)?;
                }
            }
            Inline::Image { alt, .. } => {
                if !alt.is_empty() {
                    self.write_inline_nodes(alt, output)?;
                }
            }
            Inline::InlineCode(code) => output.push_str(code),
            Inline::HardBreak => output.push_str("\n"),
            Inline::SoftBreak => output.push(' '),
            Inline::Autolink { url, .. } => output.push_str(url),
            Inline::HtmlElement(element) => {
                for child in &element.children {
                    self.write_ir_node(child, output)?;
                }
            }
            Inline::Verbatim(text) => output.push_str(text),
            Inline::EmbeddedBlock(block) => {
                self.write_block(block, output)?;
            }
            Inline::Comment(_) => {}
            Inline::UnsupportedCustom => {}
        }

        Ok(())
    }
}

impl IrFormatWriter for TextWriter {
    fn write_ir_eco(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        self.write_document(document, output)
    }

    fn write_ir_vec(&mut self, document: &ir::Document) -> Result<Vec<u8>> {
        let mut output = EcoString::new();
        self.write_ir_eco(document, &mut output)?;
        Ok(output.as_str().as_bytes().to_vec())
    }
}
