//! Semantic intermediate representation for typlite.
//!
//! This IR is independent from the CommonMark AST and is the shared document
//! model across all output formats.

use std::path::PathBuf;

use ecow::EcoString;

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

