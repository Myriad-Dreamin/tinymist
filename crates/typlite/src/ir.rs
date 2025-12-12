//! Semantic intermediate representation for typlite.
//!
//! This IR is independent from the CommonMark AST and is intended to be the
//! shared document model for non-Markdown targets.

use std::path::PathBuf;

use ecow::EcoString;

use crate::common::{
    AlertNode, CenterNode, ExternalFrameNode, FigureNode, HighlightNode, InlineNode, VerbatimNode,
};

/// A converted document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// Block-level elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Document(Vec<Block>),
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    ThematicBreak,
    BlockQuote(Vec<Block>),
    OrderedList {
        start: u32,
        items: Vec<ListItem>,
    },
    UnorderedList(Vec<ListItem>),
    Table(Table),
    CodeBlock {
        language: Option<EcoString>,
        content: EcoString,
        block_type: CodeBlockType,
    },
    HtmlBlock(EcoString),
    HtmlElement(HtmlElement),
    Figure {
        body: Box<Block>,
        caption: Vec<Inline>,
    },
    ExternalFrame(ExternalFrame),
    Center(Box<Block>),
    Alert {
        class: EcoString,
        content: Vec<Block>,
    },
}

/// Inline-level elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(EcoString),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Group(Vec<Inline>),
    InlineCode(EcoString),
    Link {
        url: EcoString,
        title: Option<EcoString>,
        content: Vec<Inline>,
    },
    ReferenceLink {
        label: EcoString,
        content: Vec<Inline>,
    },
    Image {
        url: EcoString,
        title: Option<EcoString>,
        alt: Vec<Inline>,
    },
    Autolink {
        url: EcoString,
        is_email: bool,
    },
    HardBreak,
    SoftBreak,
    HtmlElement(HtmlElement),
    Highlight(Vec<Inline>),
    Verbatim(EcoString),
    EmbeddedBlock(Box<Block>),
    UnsupportedCustom,
}

/// A unified node, used only where block/inline can mix (e.g. HTML children).
#[derive(Debug, Clone, PartialEq)]
pub enum IrNode {
    Block(Block),
    Inline(Inline),
}

/// Code block type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeBlockType {
    Indented,
    #[default]
    Fenced,
}

/// A list item.
#[derive(Debug, Clone, PartialEq)]
pub enum ListItem {
    Ordered {
        number: Option<u32>,
        content: Vec<Block>,
    },
    Unordered {
        content: Vec<Block>,
    },
}

/// Table column alignment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TableAlignment {
    #[default]
    Left,
    Center,
    Right,
    None,
}

/// Table row classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableRowKind {
    Head,
    Body,
    Foot,
}

/// Table cell classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCellKind {
    Header,
    Data,
}

/// Represents a single table cell.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub kind: TableCellKind,
    pub colspan: usize,
    pub rowspan: usize,
    pub content: Vec<IrNode>,
    pub align: Option<TableAlignment>,
}

/// Represents a logical row inside a table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub kind: TableRowKind,
    pub cells: Vec<TableCell>,
}

/// Table block.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub columns: usize,
    pub rows: Vec<TableRow>,
    pub alignments: Vec<TableAlignment>,
}

/// HTML attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlAttribute {
    pub name: EcoString,
    pub value: EcoString,
}

/// HTML element.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlElement {
    pub tag: EcoString,
    pub attributes: Vec<HtmlAttribute>,
    pub children: Vec<IrNode>,
    pub self_closing: bool,
}

/// External frame (SVG stored externally).
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalFrame {
    pub file_path: PathBuf,
    pub alt_text: EcoString,
    pub svg: String,
}

impl Document {
    /// Convert a CommonMark AST node into semantic IR.
    pub fn from_cmark(node: &cmark_writer::ast::Node) -> Self {
        use cmark_writer::ast::Node as CmarkNode;
        match node {
            CmarkNode::Document(children) => Document {
                blocks: children.iter().filter_map(convert_block).collect(),
            },
            other => Document {
                blocks: convert_block(other).into_iter().collect(),
            },
        }
    }
}

fn convert_block(node: &cmark_writer::ast::Node) -> Option<Block> {
    use cmark_writer::ast::Node as CmarkNode;

    match node {
        CmarkNode::Document(children) => Some(Block::Document(
            children.iter().filter_map(convert_block).collect(),
        )),
        CmarkNode::Paragraph(inlines) => Some(Block::Paragraph(
            inlines.iter().filter_map(convert_inline).collect(),
        )),
        CmarkNode::Heading { level, content, .. } => Some(Block::Heading {
            level: *level,
            content: content.iter().filter_map(convert_inline).collect(),
        }),
        CmarkNode::ThematicBreak => Some(Block::ThematicBreak),
        CmarkNode::BlockQuote(content) => Some(Block::BlockQuote(
            content.iter().filter_map(convert_block).collect(),
        )),
        CmarkNode::OrderedList { start, items } => Some(Block::OrderedList {
            start: *start,
            items: items.iter().filter_map(convert_list_item).collect(),
        }),
        CmarkNode::UnorderedList(items) => Some(Block::UnorderedList(
            items.iter().filter_map(convert_list_item).collect(),
        )),
        CmarkNode::Table {
            columns,
            rows,
            alignments,
        } => Some(Block::Table(convert_table(*columns, rows, alignments))),
        CmarkNode::CodeBlock {
            language,
            content,
            block_type,
        } => Some(Block::CodeBlock {
            language: language.clone(),
            content: content.clone(),
            block_type: match block_type {
                cmark_writer::ast::CodeBlockType::Indented => CodeBlockType::Indented,
                cmark_writer::ast::CodeBlockType::Fenced => CodeBlockType::Fenced,
            },
        }),
        CmarkNode::HtmlBlock(html) => Some(Block::HtmlBlock(html.clone())),
        CmarkNode::HtmlElement(element) => Some(Block::HtmlElement(convert_html_element(element))),
        CmarkNode::Custom(_) if node.is_custom_type::<FigureNode>() => {
            let fig = node.as_custom_type::<FigureNode>().unwrap();
            Some(Block::Figure {
                body: Box::new(convert_block(&fig.body).unwrap_or(Block::Paragraph(Vec::new()))),
                caption: fig.caption.iter().filter_map(convert_inline).collect(),
            })
        }
        CmarkNode::Custom(_) if node.is_custom_type::<ExternalFrameNode>() => {
            let frame = node.as_custom_type::<ExternalFrameNode>().unwrap();
            Some(Block::ExternalFrame(ExternalFrame {
                file_path: frame.file_path.clone(),
                alt_text: frame.alt_text.clone(),
                svg: frame.svg.clone(),
            }))
        }
        CmarkNode::Custom(_) if node.is_custom_type::<CenterNode>() => {
            let center = node.as_custom_type::<CenterNode>().unwrap();
            Some(Block::Center(Box::new(
                convert_block(&center.node).unwrap_or(Block::Paragraph(Vec::new())),
            )))
        }
        CmarkNode::Custom(_) if node.is_custom_type::<AlertNode>() => {
            let alert = node.as_custom_type::<AlertNode>().unwrap();
            let content = alert.content.iter().filter_map(convert_block).collect();
            Some(Block::Alert {
                class: alert.class.clone(),
                content,
            })
        }
        // Non-block custom nodes in block position are ignored.
        CmarkNode::Custom(_) => None,
        // Block-level nodes that typlite doesn't currently target.
        CmarkNode::LinkReferenceDefinition { .. } => None,
        // Treat inline nodes that slipped into block context as a paragraph.
        CmarkNode::Text(_)
        | CmarkNode::Emphasis(_)
        | CmarkNode::Strong(_)
        | CmarkNode::Strikethrough(_)
        | CmarkNode::InlineCode(_)
        | CmarkNode::Link { .. }
        | CmarkNode::ReferenceLink { .. }
        | CmarkNode::Image { .. }
        | CmarkNode::Autolink { .. }
        | CmarkNode::SoftBreak
        | CmarkNode::HardBreak => Some(Block::Paragraph(
            std::iter::once(node).filter_map(convert_inline).collect(),
        )),
        // Fallback
        _ => None,
    }
}

fn convert_list_item(item: &cmark_writer::ast::ListItem) -> Option<ListItem> {
    use cmark_writer::ast::ListItem as CmarkListItem;
    match item {
        CmarkListItem::Ordered { number, content } => Some(ListItem::Ordered {
            number: *number,
            content: content.iter().filter_map(convert_block).collect(),
        }),
        CmarkListItem::Unordered { content } => Some(ListItem::Unordered {
            content: content.iter().filter_map(convert_block).collect(),
        }),
        _ => None,
    }
}

fn convert_inline(node: &cmark_writer::ast::Node) -> Option<Inline> {
    use cmark_writer::ast::Node as CmarkNode;
    match node {
        CmarkNode::Text(text) => Some(Inline::Text(text.clone())),
        CmarkNode::Emphasis(content) => Some(Inline::Emphasis(
            content.iter().filter_map(convert_inline).collect(),
        )),
        CmarkNode::Strong(content) => Some(Inline::Strong(
            content.iter().filter_map(convert_inline).collect(),
        )),
        CmarkNode::Strikethrough(content) => Some(Inline::Strikethrough(
            content.iter().filter_map(convert_inline).collect(),
        )),
        CmarkNode::InlineCode(code) => Some(Inline::InlineCode(code.clone())),
        CmarkNode::Link {
            url,
            title,
            content,
        } => Some(Inline::Link {
            url: url.clone(),
            title: title.clone(),
            content: content.iter().filter_map(convert_inline).collect(),
        }),
        CmarkNode::ReferenceLink { label, content } => Some(Inline::ReferenceLink {
            label: label.clone(),
            content: content.iter().filter_map(convert_inline).collect(),
        }),
        CmarkNode::Image { url, title, alt } => Some(Inline::Image {
            url: url.clone(),
            title: title.clone(),
            alt: alt.iter().filter_map(convert_inline).collect(),
        }),
        CmarkNode::Autolink { url, is_email } => Some(Inline::Autolink {
            url: url.clone(),
            is_email: *is_email,
        }),
        CmarkNode::HardBreak => Some(Inline::HardBreak),
        CmarkNode::SoftBreak => Some(Inline::SoftBreak),
        CmarkNode::HtmlElement(element) => Some(Inline::HtmlElement(convert_html_element(element))),
        CmarkNode::HtmlBlock(html) => Some(Inline::Verbatim(html.clone())),
        CmarkNode::Custom(_) if node.is_custom_type::<HighlightNode>() => {
            let hl = node.as_custom_type::<HighlightNode>().unwrap();
            Some(Inline::Highlight(
                hl.content.iter().filter_map(convert_inline).collect(),
            ))
        }
        CmarkNode::Custom(_) if node.is_custom_type::<InlineNode>() => {
            let group = node.as_custom_type::<InlineNode>().unwrap();
            // Flatten to a group of inline elements.
            let mut items = Vec::new();
            for child in &group.content {
                if let Some(inline) = convert_inline(child) {
                    items.push(inline);
                }
            }
            Some(Inline::Group(items))
        }
        CmarkNode::Custom(_) if node.is_custom_type::<VerbatimNode>() => {
            let verb = node.as_custom_type::<VerbatimNode>().unwrap();
            Some(Inline::Verbatim(verb.content.clone()))
        }
        // Block custom nodes in inline position are ignored.
        CmarkNode::Custom(_) => Some(Inline::UnsupportedCustom),
        // If a block node leaks into an inline context, keep it as an embedded block.
        other => convert_block(other).map(|block| Inline::EmbeddedBlock(Box::new(block))),
    }
}

fn convert_html_element(element: &cmark_writer::ast::HtmlElement) -> HtmlElement {
    HtmlElement {
        tag: element.tag.clone(),
        attributes: element
            .attributes
            .iter()
            .map(|attr| HtmlAttribute {
                name: attr.name.clone(),
                value: attr.value.clone(),
            })
            .collect(),
        children: element.children.iter().filter_map(convert_any).collect(),
        self_closing: element.self_closing,
    }
}

fn convert_any(node: &cmark_writer::ast::Node) -> Option<IrNode> {
    if node.is_block() {
        convert_block(node).map(IrNode::Block)
    } else {
        convert_inline(node)
            .map(IrNode::Inline)
            .or_else(|| convert_block(node).map(IrNode::Block))
    }
}

fn convert_table(
    columns: usize,
    rows: &[cmark_writer::ast::TableRow],
    alignments: &[cmark_writer::ast::TableAlignment],
) -> Table {
    let alignments = alignments
        .iter()
        .map(|a| match a {
            cmark_writer::ast::TableAlignment::Left => TableAlignment::Left,
            cmark_writer::ast::TableAlignment::Center => TableAlignment::Center,
            cmark_writer::ast::TableAlignment::Right => TableAlignment::Right,
            cmark_writer::ast::TableAlignment::None => TableAlignment::None,
        })
        .collect();

    let rows = rows
        .iter()
        .map(|row| TableRow {
            kind: match row.kind {
                cmark_writer::ast::TableRowKind::Head => TableRowKind::Head,
                cmark_writer::ast::TableRowKind::Body => TableRowKind::Body,
                cmark_writer::ast::TableRowKind::Foot => TableRowKind::Foot,
            },
            cells: row
                .cells
                .iter()
                .map(|cell| TableCell {
                    kind: match cell.kind {
                        cmark_writer::ast::TableCellKind::Header => TableCellKind::Header,
                        cmark_writer::ast::TableCellKind::Data => TableCellKind::Data,
                    },
                    colspan: cell.colspan,
                    rowspan: cell.rowspan,
                    content: convert_cell_content(&cell.content),
                    align: cell.align.as_ref().map(|a| match a {
                        cmark_writer::ast::TableAlignment::Left => TableAlignment::Left,
                        cmark_writer::ast::TableAlignment::Center => TableAlignment::Center,
                        cmark_writer::ast::TableAlignment::Right => TableAlignment::Right,
                        cmark_writer::ast::TableAlignment::None => TableAlignment::None,
                    }),
                })
                .collect(),
        })
        .collect();

    Table {
        columns,
        rows,
        alignments,
    }
}

fn convert_cell_content(node: &cmark_writer::ast::Node) -> Vec<IrNode> {
    use cmark_writer::ast::Node as CmarkNode;
    match node {
        CmarkNode::Paragraph(inlines) => inlines.iter().filter_map(convert_any).collect(),
        CmarkNode::Document(children) => children.iter().filter_map(convert_any).collect(),
        other => convert_any(other).into_iter().collect(),
    }
}
