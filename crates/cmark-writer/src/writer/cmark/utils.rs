use crate::ast::Node;

/// Check if the inline node contains a newline character recursively.
pub(crate) fn node_contains_newline(node: &Node) -> bool {
    match node {
        Node::Text(s) | Node::InlineCode(s) => s.contains('\n'),
        Node::Emphasis(children) | Node::Strong(children) => {
            children.iter().any(node_contains_newline)
        }
        #[cfg(feature = "gfm")]
        Node::Strikethrough(children) => children.iter().any(node_contains_newline),
        Node::HtmlElement(element) => element.children.iter().any(node_contains_newline),
        Node::Link { content, .. } => content.iter().any(node_contains_newline),
        Node::Image { alt, .. } => alt.iter().any(node_contains_newline),
        Node::SoftBreak | Node::HardBreak => true,
        Node::Custom(_) => false,
        _ => false,
    }
}

/// Check if a table contains any block-level elements in headers or cells.
pub(crate) fn table_contains_block_elements(headers: &[Node], rows: &[Vec<Node>]) -> bool {
    if headers.iter().any(Node::is_block) {
        return true;
    }

    rows.iter().any(|row| row.iter().any(Node::is_block))
}

/// Escapes a string using the specified escaping strategy.
pub(crate) fn escape_str<E: Escapes>(s: &str) -> std::borrow::Cow<'_, str> {
    if E::str_needs_escaping(s) {
        std::borrow::Cow::Owned(format!("{}", Escaped::<E>::new(s)))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// A trait for character escaping behavior.
pub(crate) trait Escapes {
    /// Checks if the string needs escaping.
    fn str_needs_escaping(s: &str) -> bool;

    /// Returns true if the character needs to be escaped.
    fn char_needs_escaping(c: char) -> bool;

    /// Returns the escaped version of a character (if needed).
    fn escape_char(c: char) -> Option<&'static str>;
}

/// Markdown escaping implementation for CommonMark.
pub(crate) struct CommonMarkEscapes;

impl Escapes for CommonMarkEscapes {
    fn str_needs_escaping(s: &str) -> bool {
        s.chars().any(Self::char_needs_escaping)
    }

    fn char_needs_escaping(c: char) -> bool {
        matches!(c, '\\' | '*' | '_' | '[' | ']' | '<' | '>' | '`')
    }

    fn escape_char(c: char) -> Option<&'static str> {
        match c {
            '\\' => Some(r"\\"),
            '*' => Some(r"\*"),
            '_' => Some(r"\_"),
            '[' => Some(r"\["),
            ']' => Some(r"\]"),
            '<' => Some(r"\<"),
            '>' => Some(r"\>"),
            '`' => Some(r"\`"),
            _ => None,
        }
    }
}

/// A wrapper for efficient escaping.
pub(crate) struct Escaped<'a, E: Escapes> {
    inner: &'a str,
    _phantom: std::marker::PhantomData<E>,
}

impl<'a, E: Escapes> Escaped<'a, E> {
    /// Create a new Escaped wrapper.
    pub fn new(s: &'a str) -> Self {
        Self {
            inner: s,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<E: Escapes> std::fmt::Display for Escaped<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.inner.chars() {
            if E::char_needs_escaping(c) {
                f.write_str(E::escape_char(c).unwrap())?;
            } else {
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}
