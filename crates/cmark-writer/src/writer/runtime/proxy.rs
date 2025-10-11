use crate::ast::Node;
use crate::error::{WriteError, WriteResult};
use crate::options::WriterOptions;
use crate::writer::cmark::CommonMarkWriter;
use ecow::EcoString;

/// Proxy that exposes a restricted API for block-level custom node rendering.
pub struct BlockWriterProxy<'a> {
    inner: &'a mut CommonMarkWriter,
}

impl<'a> BlockWriterProxy<'a> {
    pub(crate) fn new(inner: &'a mut CommonMarkWriter) -> Self {
        Self { inner }
    }

    /// Write a block-level node through the underlying writer.
    pub fn write_block(&mut self, node: &Node) -> WriteResult<()> {
        if !node.is_block() {
            return Err(WriteError::InvalidStructure(
                "Block writer expected a block-level node".into(),
            ));
        }
        self.inner.write(node)
    }

    /// Write an inline node while remaining in a block context.
    pub fn write_inline(&mut self, node: &Node) -> WriteResult<()> {
        if node.is_block() {
            return Err(WriteError::InvalidStructure(
                "Inline content expected an inline node".into(),
            ));
        }
        self.inner.write(node)
    }

    /// Write a collection of inline nodes.
    pub fn write_inline_nodes(&mut self, nodes: &[Node]) -> WriteResult<()> {
        for node in nodes {
            self.write_inline(node)?;
        }
        Ok(())
    }

    /// Write raw text into the buffer.
    pub fn write_str(&mut self, text: &str) -> WriteResult<()> {
        self.inner.write_str(text)
    }

    /// Write a single character into the buffer.
    pub fn write_char(&mut self, ch: char) -> WriteResult<()> {
        self.inner.write_char(ch)
    }

    /// Ensure the buffer ends with a newline.
    pub fn ensure_trailing_newline(&mut self) -> WriteResult<()> {
        self.inner.ensure_trailing_newline()
    }

    /// Ensure there is a blank line at the end of the buffer.
    pub fn ensure_blank_line(&mut self) -> WriteResult<()> {
        self.inner.ensure_blank_line()
    }

    /// Capture the output produced inside the closure using a fresh block proxy.
    pub fn capture_block<F>(&mut self, f: F) -> WriteResult<EcoString>
    where
        F: FnOnce(&mut BlockWriterProxy<'_>) -> WriteResult<()>,
    {
        self.inner.capture_with_buffer(|inner| {
            let mut proxy = BlockWriterProxy::new(inner);
            f(&mut proxy)
        })
    }

    /// Capture output produced in an inline context.
    pub fn capture_inline<F>(&mut self, f: F) -> WriteResult<EcoString>
    where
        F: FnOnce(&mut InlineWriterProxy<'_>) -> WriteResult<()>,
    {
        self.inner.capture_with_buffer(|inner| {
            let mut proxy = InlineWriterProxy::new(inner);
            f(&mut proxy)
        })
    }

    /// Temporarily modify writer options while executing the provided closure.
    pub fn with_temporary_options<F, R, G>(&mut self, modify: F, mut f: G) -> WriteResult<R>
    where
        F: FnOnce(&mut WriterOptions),
        G: FnMut(&mut BlockWriterProxy<'_>) -> WriteResult<R>,
    {
        let original = self.inner.options.clone();
        modify(&mut self.inner.options);
        let result = f(self);
        self.inner.options = original;
        result
    }
}

/// Proxy that exposes a restricted API for inline custom node rendering.
pub struct InlineWriterProxy<'a> {
    inner: &'a mut CommonMarkWriter,
}

impl<'a> InlineWriterProxy<'a> {
    pub(crate) fn new(inner: &'a mut CommonMarkWriter) -> Self {
        Self { inner }
    }

    /// Write an inline node through the underlying writer.
    pub fn write_inline(&mut self, node: &Node) -> WriteResult<()> {
        if node.is_block() {
            return Err(WriteError::InvalidStructure(
                "Inline writer cannot emit block-level nodes".into(),
            ));
        }
        self.inner.write(node)
    }

    /// Write a collection of inline nodes.
    pub fn write_inline_nodes(&mut self, nodes: &[Node]) -> WriteResult<()> {
        for node in nodes {
            self.write_inline(node)?;
        }
        Ok(())
    }

    /// Write raw text into the buffer.
    pub fn write_str(&mut self, text: &str) -> WriteResult<()> {
        self.inner.write_str(text)
    }

    /// Write a single character into the buffer.
    pub fn write_char(&mut self, ch: char) -> WriteResult<()> {
        self.inner.write_char(ch)
    }

    /// Capture inline output produced inside the closure.
    pub fn capture_inline<F>(&mut self, f: F) -> WriteResult<EcoString>
    where
        F: FnOnce(&mut InlineWriterProxy<'_>) -> WriteResult<()>,
    {
        self.inner.capture_with_buffer(|inner| {
            let mut proxy = InlineWriterProxy::new(inner);
            f(&mut proxy)
        })
    }

    /// Temporarily modify writer options while executing the provided closure.
    pub fn with_temporary_options<F, R, G>(&mut self, modify: F, mut f: G) -> WriteResult<R>
    where
        F: FnOnce(&mut WriterOptions),
        G: FnMut(&mut InlineWriterProxy<'_>) -> WriteResult<R>,
    {
        let original = self.inner.options.clone();
        modify(&mut self.inner.options);
        let result = f(self);
        self.inner.options = original;
        result
    }
}
