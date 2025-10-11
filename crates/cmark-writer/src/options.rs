//! CommonMark formatting options.
//!
//! This module provides configuration options for the CommonMark writer.

use crate::writer::html::HtmlWriterOptions;
#[cfg(feature = "gfm")]
use ecow::EcoString;

/// CommonMark formatting options
#[derive(Debug, Clone)]
pub struct WriterOptions {
    /// Whether to enable strict mode (strictly following CommonMark specification)
    pub strict: bool,
    /// Hard break mode (true uses two spaces followed by a newline, false uses backslash followed by a newline)
    pub hard_break_spaces: bool,
    /// Number of spaces to use for indentation levels
    pub indent_spaces: usize,
    /// Character to use for unordered list markers (-, +, or *)
    pub list_marker: char,
    /// Character to use for thematic breaks (-, *, or _)
    pub thematic_break_char: char,
    /// Character to use for emphasis (_, or *)
    pub emphasis_char: char,
    /// Character to use for strong emphasis (_, or *)
    pub strong_char: char,
    /// Whether to escape special characters in text content
    pub escape_special_chars: bool,
    /// Whether to trim trailing hard breaks from paragraphs
    pub trim_paragraph_trailing_hard_breaks: bool,

    /// Whether to enable GitHub Flavored Markdown (GFM) extensions
    #[cfg(feature = "gfm")]
    pub enable_gfm: bool,

    /// Whether to enable GFM strikethrough syntax
    #[cfg(feature = "gfm")]
    pub gfm_strikethrough: bool,

    /// Whether to enable GFM task lists
    #[cfg(feature = "gfm")]
    pub gfm_tasklists: bool,

    /// Whether to enable GFM tables with alignment
    #[cfg(feature = "gfm")]
    pub gfm_tables: bool,

    /// Whether to enable GFM autolinks without angle brackets
    #[cfg(feature = "gfm")]
    pub gfm_autolinks: bool,

    /// List of disallowed HTML tag names in GFM mode
    #[cfg(feature = "gfm")]
    pub gfm_disallowed_html_tags: Vec<EcoString>,

    /// HTML writer options for rendering HtmlElement nodes
    /// If None, options will be automatically derived from CommonMark options
    pub html_writer_options: Option<HtmlWriterOptions>,
}

impl Default for WriterOptions {
    fn default() -> Self {
        Self {
            strict: true,
            hard_break_spaces: false,
            indent_spaces: 4,
            list_marker: '-',
            thematic_break_char: '-',
            emphasis_char: '_',
            strong_char: '*',
            escape_special_chars: false,
            trim_paragraph_trailing_hard_breaks: true,

            #[cfg(feature = "gfm")]
            enable_gfm: false,

            #[cfg(feature = "gfm")]
            gfm_strikethrough: false,

            #[cfg(feature = "gfm")]
            gfm_tasklists: false,

            #[cfg(feature = "gfm")]
            gfm_tables: false,

            #[cfg(feature = "gfm")]
            gfm_autolinks: false,

            #[cfg(feature = "gfm")]
            gfm_disallowed_html_tags: vec![
                "title".into(),
                "textarea".into(),
                "style".into(),
                "xmp".into(),
                "iframe".into(),
                "noembed".into(),
                "noframes".into(),
                "script".into(),
                "plaintext".into(),
            ],

            html_writer_options: None,
        }
    }
}

impl WriterOptions {
    /// Set custom HTML writer options for rendering HtmlElement nodes
    pub fn html_writer_options(mut self, options: Option<HtmlWriterOptions>) -> Self {
        self.html_writer_options = options;
        self
    }
}

/// Builder for WriterOptions
pub struct WriterOptionsBuilder {
    options: WriterOptions,
}

impl WriterOptionsBuilder {
    /// Create a new WriterOptionsBuilder with default options
    pub fn new() -> Self {
        Self {
            options: WriterOptions::default(),
        }
    }

    /// Set strict mode (whether to strictly follow CommonMark specification)
    pub fn strict(mut self, strict: bool) -> Self {
        self.options.strict = strict;
        self
    }

    /// Set hard break mode (true uses two spaces followed by a newline, false uses backslash)
    pub fn hard_break_spaces(mut self, hard_break_spaces: bool) -> Self {
        self.options.hard_break_spaces = hard_break_spaces;
        self
    }

    /// Set number of spaces for indentation
    pub fn indent_spaces(mut self, indent_spaces: usize) -> Self {
        self.options.indent_spaces = indent_spaces;
        self
    }

    /// Set the marker character for unordered lists (-, +, or *)
    pub fn list_marker(mut self, marker: char) -> Self {
        if marker == '-' || marker == '+' || marker == '*' {
            self.options.list_marker = marker;
        }
        self
    }

    /// Set whether to escape special characters in text content
    pub fn escape_special_chars(mut self, escape: bool) -> Self {
        self.options.escape_special_chars = escape;
        self
    }

    /// Set whether to trim trailing hard breaks from paragraphs
    pub fn trim_paragraph_trailing_hard_breaks(mut self, trim: bool) -> Self {
        self.options.trim_paragraph_trailing_hard_breaks = trim;
        self
    }

    /// Set the character for thematic breaks (-, *, or _)
    pub fn thematic_break_char(mut self, char: char) -> Self {
        if char == '-' || char == '*' || char == '_' {
            self.options.thematic_break_char = char;
        }
        self
    }

    /// Set the character for emphasis (_, or *)
    pub fn emphasis_char(mut self, char: char) -> Self {
        if char == '_' || char == '*' {
            self.options.emphasis_char = char;
        }
        self
    }

    /// Set the character for strong emphasis (_, or *)
    pub fn strong_char(mut self, char: char) -> Self {
        if char == '_' || char == '*' {
            self.options.strong_char = char;
        }
        self
    }

    /// Enable all GitHub Flavored Markdown (GFM) extensions
    #[cfg(feature = "gfm")]
    pub fn enable_gfm(mut self) -> Self {
        self.options.enable_gfm = true;
        self.options.gfm_strikethrough = true;
        self.options.gfm_tasklists = true;
        self.options.gfm_tables = true;
        self.options.gfm_autolinks = true;
        self
    }

    /// Enable or disable GFM strikethrough syntax
    #[cfg(feature = "gfm")]
    pub fn gfm_strikethrough(mut self, enable: bool) -> Self {
        self.options.gfm_strikethrough = enable;
        if enable {
            self.options.enable_gfm = true;
        }
        self
    }

    /// Enable or disable GFM task lists
    #[cfg(feature = "gfm")]
    pub fn gfm_tasklists(mut self, enable: bool) -> Self {
        self.options.gfm_tasklists = enable;
        if enable {
            self.options.enable_gfm = true;
        }
        self
    }

    /// Enable or disable GFM tables with alignment
    #[cfg(feature = "gfm")]
    pub fn gfm_tables(mut self, enable: bool) -> Self {
        self.options.gfm_tables = enable;
        if enable {
            self.options.enable_gfm = true;
        }
        self
    }

    /// Enable or disable GFM autolinks without angle brackets
    #[cfg(feature = "gfm")]
    pub fn gfm_autolinks(mut self, enable: bool) -> Self {
        self.options.gfm_autolinks = enable;
        if enable {
            self.options.enable_gfm = true;
        }
        self
    }

    /// Set list of disallowed HTML tags in GFM mode
    #[cfg(feature = "gfm")]
    pub fn gfm_disallowed_html_tags(mut self, tags: Vec<EcoString>) -> Self {
        self.options.gfm_disallowed_html_tags = tags;
        self
    }

    /// Set custom HTML writer options for rendering HtmlElement nodes
    pub fn html_writer_options(mut self, options: Option<HtmlWriterOptions>) -> Self {
        self.options.html_writer_options = options;
        self
    }

    /// Build the WriterOptions
    pub fn build(self) -> WriterOptions {
        self.options
    }
}

impl Default for WriterOptionsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
