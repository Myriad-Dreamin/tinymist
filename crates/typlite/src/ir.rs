//! Semantic intermediate representation for typlite.
//!
//! This IR is independent from the CommonMark AST and is intended to be the
//! shared document model for non-Markdown targets.

use std::path::PathBuf;

use base64::Engine;
use ecow::EcoString;

use crate::common::{
    AlertNode, CenterNode, ExternalFrameNode, FigureNode, HighlightNode, InlineNode, VerbatimNode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmarkExportTarget {
    Markdown,
    Docx,
}

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
    Comment(EcoString),
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

    /// Convert semantic IR into a CommonMark AST document.
    pub fn to_cmark(&self) -> cmark_writer::ast::Node {
        self.to_cmark_with(CmarkExportTarget::Markdown)
    }

    pub fn to_cmark_with(&self, target: CmarkExportTarget) -> cmark_writer::ast::Node {
        cmark_writer::ast::Node::Document(
            self.blocks
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
        )
    }
}

fn block_to_cmark(block: &Block, target: CmarkExportTarget) -> cmark_writer::ast::Node {
    use cmark_writer::ast::{CodeBlockType as CmarkCodeBlockType, Node};

    match block {
        Block::Document(blocks) => Node::Document(
            blocks
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
        ),
        Block::Paragraph(inlines) => Node::Paragraph(
            inlines
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        ),
        Block::Heading { level, content } => Node::heading(
            *level,
            content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        ),
        Block::ThematicBreak => Node::ThematicBreak,
        Block::BlockQuote(content) => Node::BlockQuote(
            content
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
        ),
        Block::OrderedList { start, items } => Node::OrderedList {
            start: *start,
            items: items
                .iter()
                .filter_map(|item| list_item_to_cmark(item, target))
                .collect(),
        },
        Block::UnorderedList(items) => Node::UnorderedList(
            items
                .iter()
                .filter_map(|item| list_item_to_cmark(item, target))
                .collect(),
        ),
        Block::Table(table) => Node::Table {
            columns: table.columns,
            rows: table
                .rows
                .iter()
                .map(|row| cmark_writer::ast::TableRow {
                    kind: match row.kind {
                        TableRowKind::Head => cmark_writer::ast::TableRowKind::Head,
                        TableRowKind::Body => cmark_writer::ast::TableRowKind::Body,
                        TableRowKind::Foot => cmark_writer::ast::TableRowKind::Foot,
                    },
                    cells: row
                        .cells
                        .iter()
                        .map(|cell| {
                            let nodes: Vec<Node> = cell
                                .content
                                .iter()
                                .flat_map(|node| any_to_cmark_nodes(node, target))
                                .collect();
                            let merged = match nodes.len() {
                                0 => Node::Text(EcoString::new()),
                                1 => nodes.into_iter().next().unwrap(),
                                _ => Node::Custom(Box::new(InlineNode { content: nodes })),
                            };
                            cmark_writer::ast::TableCell {
                                kind: match cell.kind {
                                    TableCellKind::Header => {
                                        cmark_writer::ast::TableCellKind::Header
                                    }
                                    TableCellKind::Data => cmark_writer::ast::TableCellKind::Data,
                                },
                                colspan: cell.colspan,
                                rowspan: cell.rowspan,
                                content: merged,
                                align: cell.align.as_ref().map(|align| match align {
                                    TableAlignment::Left => cmark_writer::ast::TableAlignment::Left,
                                    TableAlignment::Center => {
                                        cmark_writer::ast::TableAlignment::Center
                                    }
                                    TableAlignment::Right => {
                                        cmark_writer::ast::TableAlignment::Right
                                    }
                                    TableAlignment::None => cmark_writer::ast::TableAlignment::None,
                                }),
                            }
                        })
                        .collect(),
                })
                .collect(),
            alignments: table
                .alignments
                .iter()
                .map(|align| match align {
                    TableAlignment::Left => cmark_writer::ast::TableAlignment::Left,
                    TableAlignment::Center => cmark_writer::ast::TableAlignment::Center,
                    TableAlignment::Right => cmark_writer::ast::TableAlignment::Right,
                    TableAlignment::None => cmark_writer::ast::TableAlignment::None,
                })
                .collect(),
        },
        Block::CodeBlock {
            language,
            content,
            block_type,
        } => Node::CodeBlock {
            language: language.clone(),
            content: content.clone(),
            block_type: match block_type {
                CodeBlockType::Indented => CmarkCodeBlockType::Indented,
                CodeBlockType::Fenced => CmarkCodeBlockType::Fenced,
            },
        },
        Block::HtmlBlock(html) => Node::HtmlBlock(html.clone()),
        Block::HtmlElement(element) => Node::HtmlElement(html_element_to_cmark(element, target)),
        Block::Figure { body, caption } => {
            let mut children = vec![block_to_cmark(body, target)];
            children.extend(
                caption
                    .iter()
                    .flat_map(|inline| inline_to_cmark_vec(inline, target)),
            );
            Node::HtmlElement(cmark_writer::ast::HtmlElement {
                tag: EcoString::inline("figure"),
                attributes: vec![cmark_writer::ast::HtmlAttribute {
                    name: EcoString::inline("class"),
                    value: EcoString::inline("figure"),
                }],
                children,
                self_closing: false,
            })
        }
        Block::ExternalFrame(frame) => match target {
            CmarkExportTarget::Markdown => Node::HtmlElement(cmark_writer::ast::HtmlElement {
                tag: EcoString::inline("img"),
                attributes: vec![
                    cmark_writer::ast::HtmlAttribute {
                        name: EcoString::inline("src"),
                        value: frame.file_path.display().to_string().into(),
                    },
                    cmark_writer::ast::HtmlAttribute {
                        name: EcoString::inline("alt"),
                        value: frame.alt_text.clone(),
                    },
                ],
                children: vec![],
                self_closing: true,
            }),
            CmarkExportTarget::Docx => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(frame.svg.as_bytes());
                Node::HtmlElement(cmark_writer::ast::HtmlElement {
                    tag: EcoString::inline("img"),
                    attributes: vec![
                        cmark_writer::ast::HtmlAttribute {
                            name: EcoString::inline("alt"),
                            value: EcoString::inline("typst-block"),
                        },
                        cmark_writer::ast::HtmlAttribute {
                            name: EcoString::inline("src"),
                            value: EcoString::from(format!("data:image/svg+xml;base64,{b64}")),
                        },
                    ],
                    children: vec![],
                    self_closing: true,
                })
            }
        },
        Block::Center(inner) => {
            let element = match &**inner {
                Block::Paragraph(inlines) => cmark_writer::ast::HtmlElement {
                    tag: EcoString::inline("p"),
                    attributes: vec![cmark_writer::ast::HtmlAttribute {
                        name: EcoString::inline("align"),
                        value: EcoString::inline("center"),
                    }],
                    children: inlines
                        .iter()
                        .flat_map(|inline| inline_to_cmark_vec(inline, target))
                        .collect(),
                    self_closing: false,
                },
                other => cmark_writer::ast::HtmlElement {
                    tag: EcoString::inline("p"),
                    attributes: vec![cmark_writer::ast::HtmlAttribute {
                        name: EcoString::inline("align"),
                        value: EcoString::inline("center"),
                    }],
                    children: vec![block_to_cmark(other, target)],
                    self_closing: false,
                },
            };

            match target {
                CmarkExportTarget::Markdown => {
                    let mut writer =
                        cmark_writer::HtmlWriter::with_options(cmark_writer::HtmlWriterOptions {
                            strict: false,
                            ..Default::default()
                        });
                    let _ = writer.write_node(&Node::HtmlElement(element.clone()));
                    let html = writer.into_string().unwrap_or_default();
                    Node::HtmlBlock(html.into())
                }
                CmarkExportTarget::Docx => Node::HtmlElement(element),
            }
        }
        Block::Alert { class, content } => Node::Custom(Box::new(AlertNode {
            content: content
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
            class: class.clone(),
        })),
    }
}

fn list_item_to_cmark(
    item: &ListItem,
    target: CmarkExportTarget,
) -> Option<cmark_writer::ast::ListItem> {
    use cmark_writer::ast::ListItem as CmarkListItem;
    match item {
        ListItem::Ordered { number, content } => Some(CmarkListItem::Ordered {
            number: *number,
            content: content
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
        }),
        ListItem::Unordered { content } => Some(CmarkListItem::Unordered {
            content: content
                .iter()
                .map(|block| block_to_cmark(block, target))
                .collect(),
        }),
    }
}

fn inline_to_cmark_vec(inline: &Inline, target: CmarkExportTarget) -> Vec<cmark_writer::ast::Node> {
    use cmark_writer::ast::Node;
    match inline {
        Inline::Group(content) => content
            .iter()
            .flat_map(|inline| inline_to_cmark_vec(inline, target))
            .collect(),
        Inline::Emphasis(content) => vec![Node::Emphasis(
            content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        )],
        Inline::Strong(content) => vec![Node::Strong(
            content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        )],
        Inline::Strikethrough(content) => vec![Node::Strikethrough(
            content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        )],
        Inline::Highlight(content) => match target {
            CmarkExportTarget::Markdown => {
                let mut nodes = Vec::new();
                nodes.push(Node::Text(EcoString::inline("==")));
                nodes.extend(
                    content
                        .iter()
                        .flat_map(|inline| inline_to_cmark_vec(inline, target)),
                );
                nodes.push(Node::Text(EcoString::inline("==")));
                nodes
            }
            CmarkExportTarget::Docx => vec![Node::HtmlElement(cmark_writer::ast::HtmlElement {
                tag: EcoString::inline("mark"),
                attributes: vec![],
                children: content
                    .iter()
                    .flat_map(|inline| inline_to_cmark_vec(inline, target))
                    .collect(),
                self_closing: false,
            })],
        },
        Inline::Text(text) => vec![Node::Text(text.clone())],
        Inline::InlineCode(code) => vec![Node::InlineCode(code.clone())],
        Inline::Link {
            url,
            title,
            content,
        } => vec![Node::Link {
            url: url.clone(),
            title: title.clone(),
            content: content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        }],
        Inline::ReferenceLink { label, content } => vec![Node::ReferenceLink {
            label: label.clone(),
            content: content
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        }],
        Inline::Image { url, title, alt } => vec![Node::Image {
            url: url.clone(),
            title: title.clone(),
            alt: alt
                .iter()
                .flat_map(|inline| inline_to_cmark_vec(inline, target))
                .collect(),
        }],
        Inline::Autolink { url, is_email } => vec![Node::Autolink {
            url: url.clone(),
            is_email: *is_email,
        }],
        Inline::HardBreak => vec![Node::HardBreak],
        Inline::SoftBreak => vec![Node::SoftBreak],
        Inline::HtmlElement(element) => {
            vec![Node::HtmlElement(html_element_to_cmark(element, target))]
        }
        Inline::Verbatim(text) => vec![Node::Custom(Box::new(VerbatimNode {
            content: text.clone(),
        }))],
        Inline::Comment(text) => vec![Node::Custom(Box::new(crate::common::CommentNode {
            content: text.clone(),
        }))],
        Inline::EmbeddedBlock(block) => vec![block_to_cmark(block, target)],
        Inline::UnsupportedCustom => Vec::new(),
    }
}

fn any_to_cmark_nodes(node: &IrNode, target: CmarkExportTarget) -> Vec<cmark_writer::ast::Node> {
    match node {
        IrNode::Block(block) => vec![block_to_cmark(block, target)],
        IrNode::Inline(inline) => inline_to_cmark_vec(inline, target),
    }
}

fn html_element_to_cmark(
    element: &HtmlElement,
    target: CmarkExportTarget,
) -> cmark_writer::ast::HtmlElement {
    cmark_writer::ast::HtmlElement {
        tag: element.tag.clone(),
        attributes: element
            .attributes
            .iter()
            .map(|attr| cmark_writer::ast::HtmlAttribute {
                name: attr.name.clone(),
                value: attr.value.clone(),
            })
            .collect(),
        children: element
            .children
            .iter()
            .flat_map(|node| any_to_cmark_nodes(node, target))
            .collect(),
        self_closing: element.self_closing,
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
