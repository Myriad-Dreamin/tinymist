use ecow::EcoString;

/// Options for configuring the HTML rendering process.
///
/// `HtmlWriterOptions` allows customizing how HTML is generated when rendering
/// CommonMark content. These options can be used directly with an `HtmlWriter` or
/// can be derived from a `CommonMarkWriter`'s options when rendering HTML elements
/// within CommonMark content.
///
/// # Example
///
/// ```rust
/// use cmark_writer::{HtmlWriter, HtmlWriterOptions};
///
/// // Create custom HTML rendering options
/// let options = HtmlWriterOptions {
///     strict: true,
///     code_block_language_class_prefix: Some("language-".into()),
///     #[cfg(feature = "gfm")]
///     enable_gfm: true,
///     #[cfg(feature = "gfm")]
///     gfm_disallowed_html_tags: vec!["script".into()],
/// };
///
/// // Use the options with an HtmlWriter
/// let mut writer = HtmlWriter::with_options(options);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlWriterOptions {
    /// A prefix for the class name applied to fenced code blocks.
    /// For example, if set to "lang-", a Rust code block might get class "lang-rust".
    /// If None, no language class is added.
    pub code_block_language_class_prefix: Option<EcoString>,

    /// Enables GFM-specific HTML rendering behaviors.
    #[cfg(feature = "gfm")]
    pub enable_gfm: bool,
    /// A list of HTML tags that should be rendered as text when GFM is enabled.
    #[cfg(feature = "gfm")]
    pub gfm_disallowed_html_tags: Vec<EcoString>,

    /// Determines if HTML parsing/rendering errors should be strict (panic/Err) or lenient (warn and attempt to recover/textualize).
    pub strict: bool,
}

impl Default for HtmlWriterOptions {
    fn default() -> Self {
        Self {
            code_block_language_class_prefix: Some("language-".into()),
            #[cfg(feature = "gfm")]
            enable_gfm: false, // Default to false, cmark.rs options should override
            #[cfg(feature = "gfm")]
            gfm_disallowed_html_tags: Vec::new(), // Default to empty
            strict: true, // Default to strict for HTML, can be overridden by cmark.rs options
        }
    }
}

impl HtmlWriterOptions {
    /// Sets the code block language class prefix.
    pub fn with_code_block_prefix<S: Into<EcoString>>(mut self, prefix: Option<S>) -> Self {
        self.code_block_language_class_prefix = prefix.map(|p| p.into());
        self
    }

    /// Sets strict mode.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Sets the code block language class prefix.
    pub fn set_code_block_prefix<S: Into<EcoString>>(&mut self, prefix: Option<S>) {
        self.code_block_language_class_prefix = prefix.map(|p| p.into());
    }

    /// Sets strict mode.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    /// Enables GFM-specific HTML rendering behaviors.
    #[cfg(feature = "gfm")]
    pub fn with_gfm_enabled(mut self, enable: bool) -> Self {
        self.enable_gfm = enable;
        self
    }

    /// Sets GFM-specific HTML rendering behaviors.
    #[cfg(feature = "gfm")]
    pub fn set_gfm_enabled(&mut self, enable: bool) {
        self.enable_gfm = enable;
    }

    /// Sets the list of HTML tags that should be rendered as text when GFM is enabled.
    #[cfg(feature = "gfm")]
    pub fn with_gfm_disallowed_tags(mut self, tags: Vec<EcoString>) -> Self {
        self.gfm_disallowed_html_tags = tags;
        self
    }

    /// Sets the list of HTML tags that should be rendered as text when GFM is enabled.
    #[cfg(feature = "gfm")]
    pub fn set_gfm_disallowed_tags(&mut self, tags: Vec<EcoString>) {
        self.gfm_disallowed_html_tags = tags;
    }
}
