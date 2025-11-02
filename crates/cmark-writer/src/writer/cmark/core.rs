#[cfg(feature = "gfm")]
use crate::ast::TableAlignment;
use crate::ast::{CodeBlockType, CustomNode, HeadingType, ListItem, Node};
use crate::error::{WriteError, WriteResult};
use crate::options::WriterOptions;
use crate::writer::runtime::diagnostics::{Diagnostic, DiagnosticSink, NullSink};
use crate::writer::runtime::proxy::{BlockWriterProxy, InlineWriterProxy};
use crate::writer::runtime::visitor::{walk_node, NodeHandler};
use ecow::EcoString;
use log;
use std::fmt;

use super::format::FormatPolicy;
use super::utils::node_contains_newline;

/// CommonMark writer responsible for serializing AST nodes to CommonMark text.
pub struct CommonMarkWriter {
    /// Writer options.
    pub options: WriterOptions,
    /// Buffer for storing the output text.
    pub buffer: EcoString,
    /// Formatting policy responsible for whitespace and newline management.
    format: FormatPolicy,
    /// Sink for collecting non-fatal diagnostics during rendering.
    diagnostics: Box<dyn DiagnosticSink + 'static>,
}

impl fmt::Debug for CommonMarkWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommonMarkWriter")
            .field("options", &self.options)
            .field("buffer", &self.buffer)
            .field("format", &self.format)
            .finish()
    }
}

impl CommonMarkWriter {
    /// Create a new CommonMark writer with default options.
    pub fn new() -> Self {
        Self::with_options(WriterOptions::default())
    }

    /// Create a new CommonMark writer with specified options.
    pub fn with_options(options: WriterOptions) -> Self {
        Self {
            options,
            buffer: EcoString::new(),
            format: FormatPolicy,
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

    /// Whether the writer is in strict mode.
    pub(crate) fn is_strict_mode(&self) -> bool {
        self.options.strict
    }

    /// Write an AST node as CommonMark format.
    pub fn write(&mut self, node: &Node) -> WriteResult<()> {
        walk_node(self, node)
    }

    /// Write a custom node using its implementation.
    pub(crate) fn write_custom_node(&mut self, node: &dyn CustomNode) -> WriteResult<()> {
        if node.is_block() {
            let mut proxy = BlockWriterProxy::new(self);
            node.write_block(&mut proxy)
        } else {
            let mut proxy = InlineWriterProxy::new(self);
            node.write_inline(&mut proxy)
        }
    }

    /// Check if the inline node contains a newline character and return an error if it does.
    pub(crate) fn check_no_newline(&mut self, node: &Node, context: &str) -> WriteResult<()> {
        if node_contains_newline(node) {
            if self.is_strict_mode() {
                return Err(WriteError::NewlineInInlineElement(
                    context.to_string().into(),
                ));
            } else {
                self.emit_warning(format!(
                    "Newline character found in inline element '{context}', but non-strict mode allows it (output may be affected)."
                ));
            }
        }
        Ok(())
    }

    /// Run a write operation using an isolated buffer and return the generated content.
    pub(crate) fn capture_with_buffer<F>(&mut self, f: F) -> WriteResult<EcoString>
    where
        F: FnOnce(&mut Self) -> WriteResult<()>,
    {
        let mut outer = EcoString::new();
        std::mem::swap(&mut self.buffer, &mut outer);

        let write_result = f(self);
        let captured = std::mem::take(&mut self.buffer);
        std::mem::swap(&mut self.buffer, &mut outer);

        write_result?;
        Ok(captured)
    }

    /// Get the generated CommonMark format text.
    pub fn into_string(self) -> EcoString {
        self.buffer
    }

    /// Write a string to the output buffer.
    pub fn write_str(&mut self, s: &str) -> WriteResult<()> {
        self.buffer.push_str(s);
        Ok(())
    }

    /// Write a character to the output buffer.
    pub fn write_char(&mut self, c: char) -> WriteResult<()> {
        self.buffer.push(c);
        Ok(())
    }

    /// Ensure content ends with a newline (for consistent handling at the end of block nodes).
    pub(crate) fn ensure_trailing_newline(&mut self) -> WriteResult<()> {
        self.format.ensure_trailing_newline(&mut self.buffer)
    }

    /// Ensure there is a blank line (two consecutive newlines) at the end of the buffer.
    pub(crate) fn ensure_blank_line(&mut self) -> WriteResult<()> {
        self.format.ensure_blank_line(&mut self.buffer)
    }

    /// Prepare spacing between consecutive nodes in a block context.
    pub(crate) fn prepare_block_sequence(
        &mut self,
        previous_was_block: bool,
        next_is_block: bool,
    ) -> WriteResult<()> {
        self.format
            .prepare_block_sequence(&mut self.buffer, previous_was_block, next_is_block)
    }

    /// Emit a warning through the diagnostic sink and logger.
    pub(crate) fn emit_warning<S: Into<EcoString>>(&mut self, message: S) {
        let message = message.into();
        self.diagnostics.emit(Diagnostic::warning(message.clone()));
        log::warn!("{message}");
    }

    /// Emit an informational message through the diagnostic sink and logger.
    pub(crate) fn emit_info<S: Into<EcoString>>(&mut self, message: S) {
        let message = message.into();
        self.diagnostics.emit(Diagnostic::info(message.clone()));
        log::info!("{message}");
    }
}

impl NodeHandler for CommonMarkWriter {
    type Error = WriteError;

    fn document(&mut self, children: &[Node]) -> WriteResult<()> {
        self.write_document(children)
    }

    fn paragraph(&mut self, content: &[Node]) -> WriteResult<()> {
        self.write_paragraph(content)
    }

    fn text(&mut self, text: &EcoString) -> WriteResult<()> {
        self.write_text_content(text)
    }

    fn emphasis(&mut self, content: &[Node]) -> WriteResult<()> {
        self.write_emphasis(content)
    }

    fn strong(&mut self, content: &[Node]) -> WriteResult<()> {
        self.write_strong(content)
    }

    fn thematic_break(&mut self) -> WriteResult<()> {
        self.write_thematic_break()
    }

    fn heading(
        &mut self,
        level: u8,
        content: &[Node],
        heading_type: &HeadingType,
    ) -> WriteResult<()> {
        self.write_heading(level, content, heading_type)
    }

    fn inline_code(&mut self, code: &EcoString) -> WriteResult<()> {
        self.write_code_content(code)
    }

    fn code_block(
        &mut self,
        language: &Option<EcoString>,
        content: &EcoString,
        block_type: &CodeBlockType,
    ) -> WriteResult<()> {
        self.write_code_block(language, content, block_type)
    }

    fn html_block(&mut self, content: &EcoString) -> WriteResult<()> {
        self.write_html_block(content)
    }

    fn html_element(&mut self, element: &crate::ast::HtmlElement) -> WriteResult<()> {
        self.write_html_element(element)
    }

    fn block_quote(&mut self, content: &[Node]) -> WriteResult<()> {
        self.write_blockquote(content)
    }

    fn unordered_list(&mut self, items: &[ListItem]) -> WriteResult<()> {
        self.write_unordered_list(items)
    }

    fn ordered_list(&mut self, start: u32, items: &[ListItem]) -> WriteResult<()> {
        self.write_ordered_list(start, items)
    }

    #[cfg(feature = "gfm")]
    fn table(
        &mut self,
        headers: &[Node],
        alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        self.write_table_with_alignment(headers, alignments, rows)
    }

    #[cfg(not(feature = "gfm"))]
    fn table(&mut self, headers: &[Node], rows: &[Vec<Node>]) -> WriteResult<()> {
        self.write_table(headers, rows)
    }

    fn link(
        &mut self,
        url: &EcoString,
        title: &Option<EcoString>,
        content: &[Node],
    ) -> WriteResult<()> {
        self.write_link(url, title, content)
    }

    fn image(
        &mut self,
        url: &EcoString,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> WriteResult<()> {
        self.write_image(url, title, alt)
    }

    fn soft_break(&mut self) -> WriteResult<()> {
        self.write_soft_break()
    }

    fn hard_break(&mut self) -> WriteResult<()> {
        self.write_hard_break()
    }

    fn autolink(&mut self, url: &EcoString, is_email: bool) -> WriteResult<()> {
        self.write_autolink(url, is_email)
    }

    #[cfg(feature = "gfm")]
    fn extended_autolink(&mut self, url: &EcoString) -> WriteResult<()> {
        self.write_extended_autolink(url)
    }

    fn link_reference_definition(
        &mut self,
        label: &EcoString,
        destination: &EcoString,
        title: &Option<EcoString>,
    ) -> WriteResult<()> {
        self.write_link_reference_definition(label, destination, title)
    }

    fn reference_link(&mut self, label: &EcoString, content: &[Node]) -> WriteResult<()> {
        self.write_reference_link(label, content)
    }

    #[cfg(feature = "gfm")]
    fn strikethrough(&mut self, content: &[Node]) -> WriteResult<()> {
        self.write_strikethrough(content)
    }

    fn custom(&mut self, node: &dyn CustomNode) -> WriteResult<()> {
        self.write_custom_node(node)
    }

    fn unsupported(&mut self, node: &Node) -> WriteResult<()> {
        self.emit_warning(format!(
            "Unsupported node type encountered and skipped: {node:?}"
        ));
        Ok(())
    }
}

impl Default for CommonMarkWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut writer = CommonMarkWriter::new();
        match writer.write(self) {
            Ok(_) => write!(f, "{}", writer.into_string()),
            Err(e) => write!(f, "Error writing Node: {e}"),
        }
    }
}
