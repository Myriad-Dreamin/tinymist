//! Document structure representation for DOCX conversion

/// Document structure representation before converting to DOCX
#[derive(Clone, Debug)]
pub enum DocxNode {
    Paragraph {
        style: Option<String>,
        content: Vec<DocxInline>,
        numbering: Option<(usize, usize)>, // numbering_id, level
    },
    Table {
        rows: Vec<Vec<Vec<DocxNode>>>, // rows, cells, content_nodes
    },
    Image {
        data: Vec<u8>,
        alt: String,
    },
}

/// Inline content representation
#[derive(Clone, Debug)]
pub enum DocxInline {
    Text(String),
    Strong(Vec<DocxInline>),
    Emphasis(Vec<DocxInline>),
    Highlight(Vec<DocxInline>),
    Strike(Vec<DocxInline>),
    Code(String),
    Hyperlink {
        url: String,
        content: Vec<DocxInline>,
    },
    Image {
        data: Vec<u8>,
    },
    LineBreak,
}