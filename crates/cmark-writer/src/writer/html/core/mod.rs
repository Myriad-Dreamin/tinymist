use super::{HtmlWriteError, HtmlWriteResult, HtmlWriterOptions};
use crate::ast::{
    CodeBlockType, CustomNode, HeadingType, HtmlElement, ListItem, Node, TableAlignment, TableRow,
};
use crate::writer::runtime::diagnostics::{Diagnostic, DiagnosticSink, NullSink};
use crate::writer::runtime::visitor::{walk_node, NodeHandler};
use ecow::EcoString;
use html_escape;
use log;
use std::fmt;

mod guard;

pub(crate) use guard::GuardedHtmlElement;
use guard::GuardedTagWriter;

/// HTML writer for serializing CommonMark AST nodes to HTML.
///
/// `HtmlWriter` provides a flexible API for generating HTML content from AST nodes. It can be used:
/// - Directly with individual nodes through methods like `write_node`
/// - For building HTML elements programmatically using the tag and attribute methods
/// - As part of the CommonMarkWriter's HTML rendering process
/// - In custom node implementations via the `html_impl=true` attribute
///
/// # Examples
///
/// ## Basic usage
///
/// ```rust
/// use cmark_writer::{HtmlWriter, Node};
///
/// let mut writer = HtmlWriter::new();
/// let para = Node::Paragraph(vec![Node::Text("Hello, world!".into())]);
/// writer.write_node(&para).unwrap();
///
/// let output = writer.into_string().unwrap();
/// assert_eq!(output, "<p>Hello, world!</p>\n");
/// ```
///
/// ## Building HTML elements manually
///
/// ```rust
/// use cmark_writer::HtmlWriter;
///
/// let mut writer = HtmlWriter::new();
///
/// // Create a custom HTML element
/// writer.start_tag("div").unwrap();
/// writer.attribute("class", "container").unwrap();
/// writer.finish_tag().unwrap();
///
/// writer.start_tag("h1").unwrap();
/// writer.finish_tag().unwrap();
/// writer.text("Welcome").unwrap();
/// writer.end_tag("h1").unwrap();
///
/// writer.end_tag("div").unwrap();
///
/// let output = writer.into_string().unwrap();
/// assert_eq!(output, "<div class=\"container\"><h1>Welcome</h1></div>");
/// ```
pub struct HtmlWriter {
    /// Writer options
    pub options: HtmlWriterOptions,
    /// Buffer for storing the output text
    pub(crate) buffer: EcoString,
    /// Whether a tag is currently opened
    tag_opened: bool,
    /// Sink for reporting non-fatal diagnostics.
    diagnostics: Box<dyn DiagnosticSink + 'static>,
}

impl fmt::Debug for HtmlWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HtmlWriter")
            .field("options", &self.options)
            .field("buffer", &self.buffer)
            .field("tag_opened", &self.tag_opened)
            .finish()
    }
}

impl Default for HtmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlWriter {
    /// Creates a new HTML writer with default options.
    pub fn new() -> Self {
        Self::with_options(HtmlWriterOptions::default())
    }

    /// Creates a new HTML writer with the specified options.
    pub fn with_options(options: HtmlWriterOptions) -> Self {
        HtmlWriter {
            options,
            buffer: EcoString::new(),
            tag_opened: false,
            diagnostics: Box::new(NullSink),
        }
    }

    /// Replace the diagnostic sink used to capture non-fatal issues.
    pub fn with_diagnostic_sink(mut self, sink: Box<dyn DiagnosticSink + 'static>) -> Self {
        self.diagnostics = sink;
        self
    }

    /// Swap the diagnostic sink on an existing writer.
    pub fn set_diagnostic_sink(&mut self, sink: Box<dyn DiagnosticSink + 'static>) {
        self.diagnostics = sink;
    }

    /// Get a mutable handle to the diagnostic sink.
    pub fn diagnostic_sink(&mut self) -> &mut dyn DiagnosticSink {
        self.diagnostics.as_mut()
    }

    pub(crate) fn emit_warning<S: Into<EcoString>>(&mut self, message: S) {
        let message = message.into();
        self.diagnostics.emit(Diagnostic::warning(message.clone()));
        log::warn!("{message}");
    }

    #[allow(dead_code)]
    pub(crate) fn emit_info<S: Into<EcoString>>(&mut self, message: S) {
        let message = message.into();
        self.diagnostics.emit(Diagnostic::info(message.clone()));
        log::info!("{message}");
    }

    #[allow(dead_code)]
    pub(crate) fn emit_debug<S: Into<EcoString>>(&mut self, message: S) {
        let message = message.into();
        self.diagnostics.emit(Diagnostic::info(message.clone()));
        log::debug!("{message}");
    }

    /// Updates the writer's options at runtime.
    pub fn set_options(&mut self, options: HtmlWriterOptions) {
        self.options = options;
    }

    /// Gets a reference to the current options.
    pub fn options(&self) -> &HtmlWriterOptions {
        &self.options
    }

    /// Gets a mutable reference to the current options.
    pub fn options_mut(&mut self) -> &mut HtmlWriterOptions {
        &mut self.options
    }

    /// Creates a new writer with modified options using a closure.
    pub fn with_modified_options<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut HtmlWriterOptions),
    {
        f(&mut self.options);
        self
    }

    /// Consumes the writer and returns the generated HTML string.
    pub fn into_string(mut self) -> HtmlWriteResult<EcoString> {
        self.ensure_tag_closed()?;
        Ok(self.buffer)
    }

    fn ensure_tag_closed(&mut self) -> HtmlWriteResult<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    /// Starts an HTML tag with the given name.
    pub fn start_tag(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.tag_opened = true;
        Ok(())
    }

    /// Adds an attribute to the currently open tag.
    pub fn attribute(&mut self, key: &str, value: &str) -> HtmlWriteResult<()> {
        if !self.tag_opened {
            return Err(HtmlWriteError::InvalidHtmlTag(
                "Cannot write attribute: no tag is currently open.".to_string(),
            ));
        }
        self.buffer.push(' ');
        self.buffer.push_str(key);
        self.buffer.push_str("=\"");
        self.buffer
            .push_str(html_escape::encode_double_quoted_attribute(value).as_ref());
        self.buffer.push('"');
        Ok(())
    }

    /// Finishes the current open tag.
    pub fn finish_tag(&mut self) -> HtmlWriteResult<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    /// Closes an HTML tag with the given name.
    pub fn end_tag(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str("</");
        self.buffer.push_str(tag_name);
        self.buffer.push('>');
        Ok(())
    }

    /// Writes text content, escaping HTML special characters.
    pub fn text(&mut self, text: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer
            .push_str(html_escape::encode_text(text).as_ref());
        Ok(())
    }

    /// Writes a self-closing tag with only a tag name.
    pub fn self_closing_tag(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    /// Finishes the current open tag as a self-closing tag.
    pub fn finish_self_closing_tag(&mut self) -> HtmlWriteResult<()> {
        if !self.tag_opened {
            return Err(HtmlWriteError::InvalidHtmlTag(
                "Cannot finish self-closing tag: no tag is currently open.".to_string(),
            ));
        }
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    /// Writes HTML content that is trusted to be well-formed and safe.
    ///
    /// Prefer this when the HTML fragment originates from the renderer itself
    /// (e.g. structural newlines or tags we synthesise). External or
    /// user-provided content should go through [`Self::write_untrusted_html`] to
    /// ensure escaping.
    pub fn write_trusted_html(&mut self, html: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str(html);
        Ok(())
    }

    /// Writes HTML content that may contain characters requiring escaping.
    ///
    /// This is a semantic alias for [`Self::text`], making call sites explicit about
    /// handling potentially untrusted content.
    pub fn write_untrusted_html(&mut self, html: &str) -> HtmlWriteResult<()> {
        self.text(html)
    }

    pub(crate) fn guard_html_element<'a>(
        &'a mut self,
        element: &HtmlElement,
    ) -> HtmlWriteResult<GuardedHtmlElement<'a>> {
        #[cfg(feature = "gfm")]
        if self.options.enable_gfm
            && self
                .options
                .gfm_disallowed_html_tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(&element.tag))
        {
            self.emit_debug(format!(
                "GFM: Textualizing disallowed HTML tag: <{}>",
                element.tag
            ));
            return Ok(GuardedHtmlElement::Textualize);
        }

        if !crate::writer::html::utils::is_safe_tag_name(&element.tag) {
            if self.options.strict {
                return Err(HtmlWriteError::InvalidHtmlTag(element.tag.to_string()));
            }

            self.emit_warning(format!(
                "Invalid HTML tag name '{}' encountered. Textualizing in non-strict mode.",
                element.tag
            ));
            return Ok(GuardedHtmlElement::Textualize);
        }

        for attr in &element.attributes {
            if !crate::writer::html::utils::is_safe_attribute_name(&attr.name) {
                if self.options.strict {
                    return Err(HtmlWriteError::InvalidHtmlAttribute(attr.name.to_string()));
                }

                self.emit_warning(format!(
                    "Invalid attribute name '{}' encountered. Textualizing element in non-strict mode.",
                    attr.name
                ));
                return Ok(GuardedHtmlElement::Textualize);
            }
        }

        self.start_tag(&element.tag)?;
        Ok(GuardedHtmlElement::Render(GuardedTagWriter::new(
            self,
            element.tag.clone(),
        )))
    }

    /// Writes an HTML fragment without additional escaping.
    ///
    /// # Deprecation
    /// Prefer using [`Self::write_trusted_html`] or [`Self::write_untrusted_html`] to make
    /// the trust boundary explicit at the call site.
    #[deprecated(
        since = "0.8.0",
        note = "Use write_trusted_html for trusted fragments or write_untrusted_html for escaping"
    )]
    pub fn raw_html(&mut self, html: &str) -> HtmlWriteResult<()> {
        self.write_trusted_html(html)
    }

    /// Writes an AST `Node` to HTML using the configured options.
    pub fn write_node(&mut self, node: &Node) -> HtmlWriteResult<()> {
        walk_node(self, node)
    }
}

impl NodeHandler for HtmlWriter {
    type Error = HtmlWriteError;

    fn document(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.write_document(children)
    }

    fn paragraph(&mut self, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_paragraph(content)
    }

    fn text(&mut self, text: &EcoString) -> HtmlWriteResult<()> {
        self.write_text(text)
    }

    fn emphasis(&mut self, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_emphasis(content)
    }

    fn strong(&mut self, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_strong(content)
    }

    fn thematic_break(&mut self) -> HtmlWriteResult<()> {
        self.write_thematic_break()
    }

    fn heading(
        &mut self,
        level: u8,
        content: &[Node],
        _heading_type: &HeadingType,
    ) -> HtmlWriteResult<()> {
        self.write_heading(level, content)
    }

    fn inline_code(&mut self, code: &EcoString) -> HtmlWriteResult<()> {
        self.write_inline_code(code)
    }

    fn code_block(
        &mut self,
        language: &Option<EcoString>,
        content: &EcoString,
        _kind: &CodeBlockType,
    ) -> HtmlWriteResult<()> {
        self.write_code_block(language, content)
    }

    fn html_block(&mut self, content: &EcoString) -> HtmlWriteResult<()> {
        self.write_html_block(content)
    }

    fn html_element(&mut self, element: &HtmlElement) -> HtmlWriteResult<()> {
        self.write_html_element(element)
    }

    fn block_quote(&mut self, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_blockquote(content)
    }

    fn unordered_list(&mut self, items: &[ListItem]) -> HtmlWriteResult<()> {
        self.write_unordered_list(items)
    }

    fn ordered_list(&mut self, start: u32, items: &[ListItem]) -> HtmlWriteResult<()> {
        self.write_ordered_list(start, items)
    }

    fn table(
        &mut self,
        columns: usize,
        rows: &[TableRow],
        alignments: &[TableAlignment],
    ) -> HtmlWriteResult<()> {
        self.write_table(columns, rows, alignments)
    }

    fn link(
        &mut self,
        url: &EcoString,
        title: &Option<EcoString>,
        content: &[Node],
    ) -> HtmlWriteResult<()> {
        self.write_link(url, title, content)
    }

    fn image(
        &mut self,
        url: &EcoString,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> HtmlWriteResult<()> {
        self.write_image(url, title, alt)
    }

    fn soft_break(&mut self) -> HtmlWriteResult<()> {
        self.write_soft_break()
    }

    fn hard_break(&mut self) -> HtmlWriteResult<()> {
        self.write_hard_break()
    }

    fn autolink(&mut self, url: &EcoString, is_email: bool) -> HtmlWriteResult<()> {
        self.write_autolink(url, is_email)
    }

    #[cfg(feature = "gfm")]
    fn extended_autolink(&mut self, url: &EcoString) -> HtmlWriteResult<()> {
        self.write_extended_autolink(url)
    }

    fn link_reference_definition(
        &mut self,
        _label: &EcoString,
        _destination: &EcoString,
        _title: &Option<EcoString>,
    ) -> HtmlWriteResult<()> {
        Ok(())
    }

    fn reference_link(&mut self, label: &EcoString, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_reference_link(label, content)
    }

    #[cfg(feature = "gfm")]
    fn strikethrough(&mut self, content: &[Node]) -> HtmlWriteResult<()> {
        self.write_strikethrough(content)
    }

    fn custom(&mut self, node: &dyn CustomNode) -> HtmlWriteResult<()> {
        node.html_write(self)
    }

    fn unsupported(&mut self, node: &Node) -> HtmlWriteResult<()> {
        #[cfg(not(feature = "gfm"))]
        if let Node::ExtendedAutolink(url) = node {
            self.emit_warning(
                format!(
                    "ExtendedAutolink encountered but GFM feature is not enabled. Rendering as text: {url}"
                ),
            );
            return self.text(url);
        }

        Err(HtmlWriteError::UnsupportedNodeType(format!("{node:?}")))
    }
}
