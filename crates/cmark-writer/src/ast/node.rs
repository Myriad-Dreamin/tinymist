//! Node definitions for the CommonMark AST.

use super::custom::CustomNode;
use super::html::HtmlElement;
use ecow::EcoString;
use std::boxed::Box;

/// Code block type according to CommonMark specification
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CodeBlockType {
    /// Indented code block - composed of one or more indented chunks, each preceded by four or more spaces
    Indented,
    /// Fenced code block - surrounded by backtick or tilde fences
    #[default]
    Fenced,
}

/// Heading type according to CommonMark specification
#[derive(Debug, Clone, PartialEq, Default)]
pub enum HeadingType {
    /// ATX Type - Beginning with #
    #[default]
    Atx,
    /// Setext Type - Underlined or overlined text
    Setext,
}

/// Table column alignment options for GFM tables
#[cfg(feature = "gfm")]
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TableAlignment {
    /// Left alignment (default)
    #[default]
    Left,
    /// Center alignment
    Center,
    /// Right alignment
    Right,
    /// No specific alignment specified
    None,
}

/// Task list item status for GFM task lists
#[cfg(feature = "gfm")]
#[derive(Debug, Clone, PartialEq)]
pub enum TaskListStatus {
    /// Checked/completed task
    Checked,
    /// Unchecked/incomplete task
    Unchecked,
}

/// Main node type, representing an element in a CommonMark document
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Root document node, contains child nodes
    Document(Vec<Node>),

    // Leaf blocks
    // Thematic breaks
    /// Thematic break (horizontal rule)
    ThematicBreak,

    // ATX headings & Setext headings
    /// Heading, contains level (1-6) and inline content
    Heading {
        /// Heading level, 1-6
        level: u8,
        /// Heading content, containing inline elements
        content: Vec<Node>,
        /// Heading type (ATX or Setext)
        heading_type: HeadingType,
    },

    // Indented code blocks & Fenced code blocks
    /// Code block, containing optional language identifier and content
    CodeBlock {
        /// Optional language identifier (None for indented code blocks, Some for fenced code blocks)
        language: Option<EcoString>,
        /// Code content
        content: EcoString,
        /// The type of code block (Indented or Fenced)
        block_type: CodeBlockType,
    },

    // HTML blocks
    /// HTML block
    HtmlBlock(EcoString),

    // Link reference definitions
    /// Link reference definition
    LinkReferenceDefinition {
        /// Link label (used for reference)
        label: EcoString,
        /// Link destination URL
        destination: EcoString,
        /// Optional link title
        title: Option<EcoString>,
    },

    // Paragraphs
    /// Paragraph node, containing inline elements
    Paragraph(Vec<Node>),

    // Blank lines - typically handled during parsing, not represented in AST

    // Container blocks
    // Block quotes
    /// Block quote, containing any block-level elements
    BlockQuote(Vec<Node>),

    // & List items and Lists
    /// Ordered list, containing starting number and list items
    OrderedList {
        /// List starting number
        start: u32,
        /// List items
        items: Vec<ListItem>,
    },

    /// Unordered list, containing list items
    UnorderedList(Vec<ListItem>),

    /// Table (extension to CommonMark)
    Table {
        /// Header cells
        headers: Vec<Node>,
        /// Column alignments for the table
        #[cfg(feature = "gfm")]
        alignments: Vec<TableAlignment>,
        /// Table rows, each row containing multiple cells
        rows: Vec<Vec<Node>>,
    },

    // Inlines
    // Code spans
    /// Inline code
    InlineCode(EcoString),

    // Emphasis and strong emphasis
    /// Emphasis (italic)
    Emphasis(Vec<Node>),

    /// Strong emphasis (bold)
    Strong(Vec<Node>),

    /// Strikethrough (GFM extension)
    Strikethrough(Vec<Node>),

    // Links
    /// Link
    Link {
        /// Link URL
        url: EcoString,
        /// Optional link title
        title: Option<EcoString>,
        /// Link text
        content: Vec<Node>,
    },

    /// Reference link
    ReferenceLink {
        /// Link reference label
        label: EcoString,
        /// Link text content (optional, if empty it's a shortcut reference)
        content: Vec<Node>,
    },

    // Images
    /// Image
    Image {
        /// Image URL
        url: EcoString,
        /// Optional image title
        title: Option<EcoString>,
        /// Alternative text, containing inline elements
        alt: Vec<Node>,
    },

    // Autolinks
    /// Autolink (URI or email wrapped in < and >)
    Autolink {
        /// Link URL
        url: EcoString,
        /// Whether this is an email autolink
        is_email: bool,
    },

    /// GFM Extended Autolink (without angle brackets, automatically detected)
    ExtendedAutolink(EcoString),

    // Raw HTML
    /// HTML inline element
    HtmlElement(HtmlElement),

    // Hard line breaks
    /// Hard break (two spaces followed by a line break, or backslash followed by a line break)
    HardBreak,

    // Soft line breaks
    /// Soft break (single line break)
    SoftBreak,

    // Textual content
    /// Plain text
    Text(EcoString),

    /// Custom node that allows users to implement their own writing behavior
    Custom(Box<dyn CustomNode>),
}

impl Default for Node {
    fn default() -> Self {
        Node::Document(vec![])
    }
}

/// List item type
#[derive(Debug, Clone, PartialEq)]
pub enum ListItem {
    /// Unordered list item
    Unordered {
        /// List item content, containing one or more block-level elements
        content: Vec<Node>,
    },
    /// Ordered list item
    Ordered {
        /// Optional item number for ordered lists, allowing manual numbering
        number: Option<u32>,
        /// List item content, containing one or more block-level elements
        content: Vec<Node>,
    },
    /// Task list item (GFM extension)
    #[cfg(feature = "gfm")]
    Task {
        /// Task completion status
        status: TaskListStatus,
        /// List item content, containing one or more block-level elements
        content: Vec<Node>,
    },
}

impl Node {
    /// Check if a node is a block-level node
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            Node::Document(_)
                // Leaf blocks
                | Node::ThematicBreak
                | Node::Heading { .. }
                | Node::CodeBlock { .. }
                | Node::HtmlBlock(_)
                | Node::LinkReferenceDefinition { .. }
                | Node::Paragraph(_)
                // Container blocks
                | Node::BlockQuote(_)
                | Node::OrderedList { .. }
                | Node::UnorderedList(_)
                | Node::Table { .. }

                | Node::Custom(_)
        )
    }

    /// Check if a node is an inline node
    pub fn is_inline(&self) -> bool {
        matches!(
            self,
            // Inlines
            // Code spans
            Node::InlineCode(_)
                // Emphasis and strong emphasis
                | Node::Emphasis(_)
                | Node::Strong(_)
                | Node::Strikethrough(_)
                // Links
                | Node::Link { .. }
                | Node::ReferenceLink { .. }
                // Images
                | Node::Image { .. }
                // Autolinks
                | Node::Autolink { .. }
                | Node::ExtendedAutolink(_)
                // Raw HTML
                | Node::HtmlElement(_)
                // Hard line breaks
                | Node::HardBreak
                // Soft line breaks
                | Node::SoftBreak
                // Textual content
                | Node::Text(_)

                | Node::Custom(_)
        )
    }
    /// Create a heading node
    ///
    /// # Arguments
    /// * `level` - Heading level (1-6)
    /// * `content` - Heading content
    ///
    /// # Returns
    /// A new heading node, default ATX type
    pub fn heading(level: u8, content: Vec<Node>) -> Self {
        Node::Heading {
            level,
            content,
            heading_type: HeadingType::default(),
        }
    }

    /// Create a code block node
    ///
    /// # Arguments
    /// * `language` - Optional language identifier
    /// * `content` - Code content
    ///
    /// # Returns
    /// A new code block node, default Fenced type
    pub fn code_block(language: Option<EcoString>, content: EcoString) -> Self {
        Node::CodeBlock {
            language,
            content,
            block_type: CodeBlockType::default(),
        }
    }

    /// Create a strikethrough node
    ///
    /// # Arguments
    /// * `content` - Content to be struck through
    ///
    /// # Returns
    /// A new strikethrough node
    pub fn strikethrough(content: Vec<Node>) -> Self {
        Node::Strikethrough(content)
    }

    /// Create a task list item
    ///
    /// # Arguments
    /// * `status` - Task completion status
    /// * `content` - Task content
    ///
    /// # Returns
    /// A new task list item
    #[cfg(feature = "gfm")]
    pub fn task_list_item(status: TaskListStatus, content: Vec<Node>) -> Self {
        Node::UnorderedList(vec![ListItem::Task { status, content }])
    }

    /// Create a table with alignment
    ///
    /// # Arguments
    /// * `headers` - Table header cells
    /// * `alignments` - Column alignments
    /// * `rows` - Table rows
    ///
    /// # Returns
    /// A new table node with alignment information
    #[cfg(feature = "gfm")]
    pub fn table_with_alignment(
        headers: Vec<Node>,
        alignments: Vec<TableAlignment>,
        rows: Vec<Vec<Node>>,
    ) -> Self {
        Node::Table {
            headers,
            alignments,
            rows,
        }
    }
    /// Check if a custom node is of a specific type, and return a reference to that type
    pub fn as_custom_type<T: CustomNode + 'static>(&self) -> Option<&T> {
        if let Node::Custom(node) = self {
            node.as_any().downcast_ref::<T>()
        } else {
            None
        }
    }

    /// Check if a node is a custom node of a specific type
    pub fn is_custom_type<T: CustomNode + 'static>(&self) -> bool {
        self.as_custom_type::<T>().is_some()
    }
}
