//! CommonMark writer implementation.
//!
//! This file contains the implementation of the CommonMarkWriter class, which serializes AST nodes to CommonMark-compliant text.

use super::processors::{
    BlockNodeProcessor, CustomNodeProcessor, InlineNodeProcessor, NodeProcessor,
};
#[cfg(feature = "gfm")]
use crate::ast::TableAlignment;
use crate::ast::{CodeBlockType, CustomNode, HeadingType, ListItem, Node};
use crate::error::{WriteError, WriteResult};
use crate::options::WriterOptions;
use ecow::EcoString;
use log;
use std::fmt::{self};

/// CommonMark writer
///
/// This struct is responsible for serializing AST nodes to CommonMark-compliant text.
#[derive(Debug)]
pub struct CommonMarkWriter {
    /// Writer options
    pub options: WriterOptions,
    /// Buffer for storing the output text
    buffer: EcoString,
}

impl CommonMarkWriter {
    /// Create a new CommonMark writer with default options
    ///
    /// # Example
    ///
    /// ```
    /// use cmark_writer::writer::CommonMarkWriter;
    /// use cmark_writer::ast::Node;
    ///
    /// let mut writer = CommonMarkWriter::new();
    /// writer.write(&Node::Text("Hello".into())).unwrap();
    /// assert_eq!(writer.into_string(), "Hello");
    /// ```
    pub fn new() -> Self {
        Self::with_options(WriterOptions::default())
    }

    /// Create a new CommonMark writer with specified options
    ///
    /// # Parameters
    ///
    /// * `options` - Custom CommonMark formatting options
    ///
    /// # Example
    ///
    /// ```
    /// use cmark_writer::writer::CommonMarkWriter;
    /// use cmark_writer::options::WriterOptions;
    ///
    /// let options = WriterOptions {
    ///     strict: true,
    ///     hard_break_spaces: false,  // Use backslash for line breaks
    ///     indent_spaces: 2,          // Use 2 spaces for indentation
    ///     ..Default::default()       // Other options can be set as needed
    /// };
    /// let writer = CommonMarkWriter::with_options(options);
    /// ```
    pub fn with_options(options: WriterOptions) -> Self {
        Self {
            options,
            buffer: EcoString::new(),
        }
    }

    /// Whether the writer is in strict mode
    pub(crate) fn is_strict_mode(&self) -> bool {
        self.options.strict
    }

    /// Apply a specific prefix to multi-line text, used for handling container node indentation
    ///
    /// # Parameters
    ///
    /// * `content` - The multi-line content to process
    /// * `prefix` - The prefix to apply to each line
    /// * `first_line_prefix` - The prefix to apply to the first line (can be different from other lines)
    ///
    /// # Returns
    ///
    /// Returns a string with applied indentation
    fn apply_prefix(
        &self,
        content: &str,
        prefix: &str,
        first_line_prefix: Option<&str>,
    ) -> EcoString {
        if content.is_empty() {
            return EcoString::new();
        }

        let mut result = EcoString::new();
        let lines: Vec<&str> = content.lines().collect();

        if !lines.is_empty() {
            let actual_prefix = first_line_prefix.unwrap_or(prefix);
            result.push_str(actual_prefix);
            result.push_str(lines[0]);
        }

        for line in &lines[1..] {
            result.push('\n');
            result.push_str(prefix);
            result.push_str(line);
        }

        result
    }

    /// Write an AST node as CommonMark format
    ///
    /// # Parameters
    ///
    /// * `node` - The AST node to write
    ///
    /// # Returns
    ///
    /// If writing succeeds, returns `Ok(())`, otherwise returns `Err(WriteError)`
    ///
    /// # Example
    ///
    /// ```
    /// use cmark_writer::writer::CommonMarkWriter;
    /// use cmark_writer::ast::Node;
    ///
    /// let mut writer = CommonMarkWriter::new();
    /// writer.write(&Node::Text("Hello".into())).unwrap();
    /// ```
    pub fn write(&mut self, node: &Node) -> WriteResult<()> {
        if let Node::Custom(_) = node {
            return CustomNodeProcessor.process(self, node);
        }

        if node.is_block() {
            BlockNodeProcessor.process(self, node)
        } else if node.is_inline() {
            InlineNodeProcessor.process(self, node)
        } else {
            log::warn!("Unsupported node type encountered and skipped: {node:?}");
            Ok(())
        }
    }

    /// Write a custom node using its implementation
    #[allow(clippy::borrowed_box)]
    pub(crate) fn write_custom_node(&mut self, node: &Box<dyn CustomNode>) -> WriteResult<()> {
        node.write(self)
    }

    /// Get context description for a node, used for error reporting
    pub(crate) fn get_context_for_node(&self, node: &Node) -> EcoString {
        match node {
            Node::Text(_) => "Text".into(),
            Node::Emphasis(_) => "Emphasis".into(),
            Node::Strong(_) => "Strong".into(),
            #[cfg(feature = "gfm")]
            Node::Strikethrough(_) => "Strikethrough".into(),
            Node::InlineCode(_) => "InlineCode".into(),
            Node::Link { .. } => "Link content".into(),
            Node::Image { .. } => "Image alt text".into(),
            Node::HtmlElement(_) => "HtmlElement content".into(),
            Node::Custom(_) => "Custom node".into(),
            _ => "Unknown inline element".into(),
        }
    }

    /// Check if the inline node contains a newline character and return an error if it does
    pub(crate) fn check_no_newline(&self, node: &Node, context: &str) -> WriteResult<()> {
        if Self::node_contains_newline(node) {
            if self.is_strict_mode() {
                return Err(WriteError::NewlineInInlineElement(
                    context.to_string().into(),
                ));
            } else {
                log::warn!(
                    "Newline character found in inline element '{context}', but non-strict mode allows it (output may be affected)."
                );
            }
        }
        Ok(())
    }

    /// Check if the inline node contains a newline character recursively
    fn node_contains_newline(node: &Node) -> bool {
        match node {
            Node::Text(s) | Node::InlineCode(s) => s.contains('\n'),
            Node::Emphasis(children) | Node::Strong(children) => {
                children.iter().any(Self::node_contains_newline)
            }
            #[cfg(feature = "gfm")]
            Node::Strikethrough(children) => children.iter().any(Self::node_contains_newline),
            Node::HtmlElement(element) => element.children.iter().any(Self::node_contains_newline),
            Node::Link { content, .. } => content.iter().any(Self::node_contains_newline),
            Node::Image { alt, .. } => alt.iter().any(Self::node_contains_newline),
            Node::SoftBreak | Node::HardBreak => true,
            // Custom nodes are handled separately
            Node::Custom(_) => false,
            _ => false,
        }
    }

    /// Check if a table contains any block-level elements in headers or cells
    fn table_contains_block_elements(headers: &[Node], rows: &[Vec<Node>]) -> bool {
        // Check headers for block elements
        if headers.iter().any(|node| node.is_block()) {
            return true;
        }

        // Check all cells in all rows for block elements
        rows.iter()
            .any(|row| row.iter().any(|node| node.is_block()))
    }

    /// Writes text content with character escaping
    pub(crate) fn write_text_content(&mut self, content: &str) -> WriteResult<()> {
        if self.options.escape_special_chars {
            let escaped = escape_str::<CommonMarkEscapes>(content);
            self.write_str(&escaped)?
        } else {
            self.write_str(content)?
        }

        Ok(())
    }

    /// Writes inline code content
    pub(crate) fn write_code_content(&mut self, content: &str) -> WriteResult<()> {
        self.write_char('`')?;
        self.write_str(content)?;
        self.write_char('`')?;
        Ok(())
    }

    /// Helper function for writing content with delimiters
    pub(crate) fn write_delimited(&mut self, content: &[Node], delimiter: &str) -> WriteResult<()> {
        self.write_str(delimiter)?;

        for node in content {
            self.write(node)?;
        }

        self.write_str(delimiter)?;
        Ok(())
    }

    /// Write a document node
    pub(crate) fn write_document(&mut self, children: &[Node]) -> WriteResult<()> {
        let mut prev_was_block = false;

        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                if prev_was_block && child.is_block() {
                    self.ensure_blank_line()?;
                } else if prev_was_block || child.is_block() {
                    self.ensure_trailing_newline()?;
                }
            }

            self.write(child)?;

            if child.is_block() {
                self.ensure_trailing_newline()?;
            }

            prev_was_block = child.is_block();
        }

        Ok(())
    }

    /// Write a heading node
    pub(crate) fn write_heading(
        &mut self,
        mut level: u8,
        content: &[Node],
        heading_type: &HeadingType,
    ) -> WriteResult<()> {
        if level == 0 || level > 6 {
            if self.is_strict_mode() {
                return Err(WriteError::InvalidHeadingLevel(level));
            } else {
                let original_level = level;
                level = level.clamp(1, 6); // Clamp level to 1-6
                log::warn!(
                    "Invalid heading level: {original_level}. Corrected to {level}. Strict mode is off."
                );
            }
        }

        match heading_type {
            // ATX heading, using # character
            HeadingType::Atx => {
                for _ in 0..level {
                    self.write_char('#')?;
                }
                self.write_char(' ')?;

                for node in content {
                    self.write(node)?;
                }

                self.write_char('\n')?;
            }

            HeadingType::Setext => {
                let mut temp_writer = CommonMarkWriter::with_options(self.options.clone());
                for node in content {
                    temp_writer.write(node)?;
                }

                let heading_text = temp_writer.into_string();

                self.write_str(&heading_text)?;
                self.write_char('\n')?;

                let underline_char = if level == 1 { '=' } else { '-' };

                let max_line_width = heading_text
                    .lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(3);

                for _ in 0..max_line_width {
                    self.write_char(underline_char)?;
                }

                self.write_char('\n')?;
            }
        }

        Ok(())
    }

    /// Write a paragraph node
    pub(crate) fn write_paragraph(&mut self, content: &[Node]) -> WriteResult<()> {
        if self.options.trim_paragraph_trailing_hard_breaks {
            let mut last_non_hard_break_index = content.len();

            while last_non_hard_break_index > 0 {
                if !matches!(content[last_non_hard_break_index - 1], Node::HardBreak) {
                    break;
                }
                last_non_hard_break_index -= 1;
            }

            for node in content.iter().take(last_non_hard_break_index) {
                self.write(node)?;
            }
        } else {
            for node in content {
                self.write(node)?;
            }
        }

        self.ensure_trailing_newline()
    }

    /// Write a blockquote node
    pub(crate) fn write_blockquote(&mut self, content: &[Node]) -> WriteResult<()> {
        // Create a temporary writer buffer to write all blockquote content
        let mut temp_writer = CommonMarkWriter::with_options(self.options.clone());

        // Write all content to temporary buffer
        for (i, node) in content.iter().enumerate() {
            if i > 0 {
                temp_writer.write_char('\n')?;
            }
            // Write all nodes uniformly
            temp_writer.write(node)?;
        }

        // Get all content
        let all_content = temp_writer.into_string();

        // Apply blockquote prefix "> " uniformly
        let prefix = "> ";
        let formatted_content = self.apply_prefix(&all_content, prefix, Some(prefix));

        // Write formatted content
        self.buffer.push_str(&formatted_content);
        Ok(())
    }

    /// Write a thematic break (horizontal rule)
    pub(crate) fn write_thematic_break(&mut self) -> WriteResult<()> {
        let char = self.options.thematic_break_char;
        self.write_str(&format!("{char}{char}{char}"))?;
        self.ensure_trailing_newline()
    }

    /// Write a code block node
    pub(crate) fn write_code_block(
        &mut self,
        language: &Option<EcoString>,
        content: &str,
        block_type: &CodeBlockType,
    ) -> WriteResult<()> {
        match block_type {
            CodeBlockType::Indented => {
                let indent = "    ";
                let indented_content = self.apply_prefix(content, indent, Some(indent));
                self.buffer.push_str(&indented_content);
            }
            CodeBlockType::Fenced => {
                let max_backticks = content
                    .chars()
                    .fold((0, 0), |(max, current), c| {
                        if c == '`' {
                            (max.max(current + 1), current + 1)
                        } else {
                            (max, 0)
                        }
                    })
                    .0;

                let fence_len = std::cmp::max(max_backticks + 1, 3);
                let fence = "`".repeat(fence_len);

                self.write_str(&fence)?;
                if let Some(lang) = language {
                    self.write_str(lang)?;
                }
                self.write_char('\n')?;

                self.buffer.push_str(content);
                if !content.ends_with('\n') {
                    self.write_char('\n')?;
                }

                self.write_str(&fence)?;
            }
        }

        Ok(())
    }

    /// Write an unordered list node
    pub(crate) fn write_unordered_list(&mut self, items: &[ListItem]) -> WriteResult<()> {
        let list_marker = self.options.list_marker;
        let prefix = format!("{list_marker} ");

        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write_char('\n')?;
            }
            self.write_list_item(item, &prefix)?;
        }

        Ok(())
    }

    /// Write an ordered list node
    pub(crate) fn write_ordered_list(&mut self, start: u32, items: &[ListItem]) -> WriteResult<()> {
        // Track the current item number
        let mut current_number = start;

        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write_char('\n')?;
            }

            match item {
                // For ordered list items, check if there's a custom number
                ListItem::Ordered { number, content: _ } => {
                    if let Some(custom_num) = number {
                        // Use custom numbering
                        let prefix = format!("{custom_num}. ");
                        self.write_list_item(item, &prefix)?;
                        // Next expected number
                        current_number = custom_num + 1;
                    } else {
                        // No custom number, use the current calculated number
                        let prefix = format!("{current_number}. ");
                        self.write_list_item(item, &prefix)?;
                        current_number += 1;
                    }
                }
                // For other types of list items, still use the current number
                _ => {
                    let prefix = format!("{current_number}. ");
                    self.write_list_item(item, &prefix)?;
                    current_number += 1;
                }
            }
        }

        Ok(())
    }

    /// Write a list item
    fn write_list_item(&mut self, item: &ListItem, prefix: &str) -> WriteResult<()> {
        match item {
            ListItem::Unordered { content } => {
                self.write_str(prefix)?;
                self.write_list_item_content(content, prefix.len())?;
            }
            ListItem::Ordered { number, content } => {
                if let Some(num) = number {
                    let custom_prefix = format!("{num}. ");
                    self.write_str(&custom_prefix)?;
                    self.write_list_item_content(content, custom_prefix.len())?;
                } else {
                    self.write_str(prefix)?;
                    self.write_list_item_content(content, prefix.len())?;
                }
            }
            #[cfg(feature = "gfm")]
            ListItem::Task { status, content } => {
                // Only use task list syntax if GFM task lists are enabled
                if self.options.gfm_tasklists {
                    let checkbox = match status {
                        crate::ast::TaskListStatus::Checked => "[x] ",
                        crate::ast::TaskListStatus::Unchecked => "[ ] ",
                    };

                    // Use the original list marker (- or number) and append the checkbox
                    let task_prefix = format!("{}{}", prefix, checkbox);
                    self.write_str(&task_prefix)?;
                    self.write_list_item_content(content, task_prefix.len())?;
                } else {
                    // If GFM task lists are disabled, just write a normal list item
                    self.write_str(prefix)?;
                    self.write_list_item_content(content, prefix.len())?;
                }
            }
        }

        Ok(())
    }

    /// Write list item content
    fn write_list_item_content(&mut self, content: &[Node], prefix_len: usize) -> WriteResult<()> {
        if content.is_empty() {
            return Ok(());
        }

        let mut temp_writer = CommonMarkWriter::with_options(self.options.clone());

        for (i, node) in content.iter().enumerate() {
            if i > 0 {
                temp_writer.write_char('\n')?;
            }

            temp_writer.write(node)?;
        }

        let all_content = temp_writer.into_string();

        let indent = " ".repeat(prefix_len);

        let formatted_content = self.apply_prefix(&all_content, &indent, Some(""));

        self.buffer.push_str(&formatted_content);

        Ok(())
    }

    /// Write a table
    pub(crate) fn write_table(&mut self, headers: &[Node], rows: &[Vec<Node>]) -> WriteResult<()> {
        // Check if table contains block elements
        if Self::table_contains_block_elements(headers, rows) {
            if self.is_strict_mode() {
                // In strict mode, fail immediately if block elements are present
                return Err(WriteError::InvalidStructure(
                    "Table contains block-level elements which are not allowed in strict mode"
                        .to_string()
                        .into(),
                ));
            } else {
                // In soft mode, fallback to HTML
                log::info!(
                    "Table contains block-level elements, falling back to HTML output in soft mode"
                );
                return self.write_table_as_html(headers, rows);
            }
        }

        // Write header
        self.write_char('|')?;
        for header in headers {
            self.check_no_newline(header, "Table Header")?;
            self.write_char(' ')?;
            self.write(header)?;
            self.write_str(" |")?;
        }
        self.write_char('\n')?;

        // Write alignment row (default to centered if no alignments provided)
        self.write_char('|')?;
        for _ in 0..headers.len() {
            self.write_str(" --- |")?;
        }
        self.write_char('\n')?;

        // Write table content
        for row in rows {
            self.write_char('|')?;
            for cell in row {
                self.check_no_newline(cell, "Table Cell")?;
                self.write_char(' ')?;
                self.write(cell)?;
                self.write_str(" |")?;
            }
            self.write_char('\n')?;
        }

        Ok(())
    }

    #[cfg(feature = "gfm")]
    /// Write a table with alignment (GFM extension)
    pub(crate) fn write_table_with_alignment(
        &mut self,
        headers: &[Node],
        alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        // Only use alignment when GFM tables are enabled
        if !self.options.gfm_tables {
            return self.write_table(headers, rows);
        }

        // Check if table contains block elements
        if Self::table_contains_block_elements(headers, rows) {
            if self.is_strict_mode() {
                // In strict mode, fail immediately if block elements are present
                return Err(WriteError::InvalidStructure(
                    "GFM table contains block-level elements which are not allowed in strict mode"
                        .to_string()
                        .into(),
                ));
            } else {
                // In soft mode, fallback to HTML
                log::info!("GFM table contains block-level elements, falling back to HTML output in soft mode");
                return self.write_table_as_html_with_alignment(headers, alignments, rows);
            }
        }

        // Write header
        self.write_char('|')?;
        for header in headers {
            self.check_no_newline(header, "Table Header")?;
            self.write_char(' ')?;
            self.write(header)?;
            self.write_str(" |")?;
        }
        self.write_char('\n')?;

        // Write alignment row

        self.write_char('|')?;

        // Use provided alignments, or default to center if not enough alignments provided
        for i in 0..headers.len() {
            let alignment = if i < alignments.len() {
                &alignments[i]
            } else {
                &TableAlignment::Center
            };

            match alignment {
                TableAlignment::Left => self.write_str(" :--- |")?,
                TableAlignment::Center => self.write_str(" :---: |")?,
                TableAlignment::Right => self.write_str(" ---: |")?,
                TableAlignment::None => self.write_str(" --- |")?,
            }
        }

        self.write_char('\n')?;

        // Write table content
        for row in rows {
            self.write_char('|')?;
            for cell in row {
                self.check_no_newline(cell, "Table Cell")?;
                self.write_char(' ')?;
                self.write(cell)?;
                self.write_str(" |")?;
            }
            self.write_char('\n')?;
        }

        Ok(())
    }

    /// Write a link
    pub(crate) fn write_link(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        content: &[Node],
    ) -> WriteResult<()> {
        for node in content {
            self.check_no_newline(node, "Link Text")?;
        }
        self.write_char('[')?;

        for node in content {
            self.write(node)?;
        }

        self.write_str("](")?;
        self.write_str(url)?;

        if let Some(title_text) = title {
            self.write_str(" \"")?;
            self.write_str(title_text)?;
            self.write_char('"')?;
        }

        self.write_char(')')?;
        Ok(())
    }

    /// Write an image
    pub(crate) fn write_image(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> WriteResult<()> {
        // Check for newlines in alt text content
        for node in alt {
            self.check_no_newline(node, "Image alt text")?;
        }

        self.write_str("![")?;

        // Write alt text content
        for node in alt {
            self.write(node)?;
        }

        self.write_str("](")?;
        self.write_str(url)?;

        if let Some(title_text) = title {
            self.write_str(" \"")?;
            self.write_str(title_text)?;
            self.write_char('"')?;
        }

        self.write_char(')')?;
        Ok(())
    }

    /// Write a soft line break
    pub(crate) fn write_soft_break(&mut self) -> WriteResult<()> {
        self.write_char('\n')?;
        Ok(())
    }

    /// Write a hard line break
    pub(crate) fn write_hard_break(&mut self) -> WriteResult<()> {
        if self.options.hard_break_spaces {
            self.write_str("  \n")?;
        } else {
            self.write_str("\\\n")?;
        }
        Ok(())
    }

    /// Write an HTML block
    pub(crate) fn write_html_block(&mut self, content: &str) -> WriteResult<()> {
        self.buffer.push_str(content);

        Ok(())
    }

    /// Write an autolink (URI or email address wrapped in < and >)
    pub(crate) fn write_autolink(&mut self, url: &str, is_email: bool) -> WriteResult<()> {
        let _ = is_email; // parameter retained for API compatibility
                          // Autolinks shouldn't contain newlines
        if url.contains('\n') {
            if self.is_strict_mode() {
                return Err(WriteError::NewlineInInlineElement(
                    "Autolink URL".to_string().into(),
                ));
            } else {
                log::warn!(
                    "Newline character found in autolink URL '{url}'. Writing it as is, which might result in an invalid link. Strict mode is off."
                );
                // Continue to write the URL as is, including the newline.
            }
        }

        // Write the autolink with < and > delimiters
        self.write_char('<')?;
        self.write_str(url)?;
        self.write_char('>')?;

        Ok(())
    }

    /// Write an extended autolink (GFM extension)
    #[cfg(feature = "gfm")]
    pub(crate) fn write_extended_autolink(&mut self, url: &str) -> WriteResult<()> {
        if !self.options.gfm_autolinks {
            // If GFM autolinks are disabled, write as plain text
            self.write_text_content(url)?;
            return Ok(());
        }

        // Autolinks shouldn't contain newlines
        if url.contains('\n') {
            if self.is_strict_mode() {
                // Or a specific gfm_autolinks_strict option if desired
                return Err(WriteError::NewlineInInlineElement(
                    "Extended Autolink URL".to_string().into(),
                ));
            } else {
                log::warn!(
                    "Newline character found in extended autolink URL '{}'. Writing it as is, which might result in an invalid link. Strict mode is off.",
                    url
                );
                // Continue to write the URL as is, including the newline.
            }
        }

        // Just write the URL as plain text for extended autolinks (no angle brackets)
        self.write_str(url)?;

        Ok(())
    }

    /// Write a link reference definition
    pub(crate) fn write_link_reference_definition(
        &mut self,
        label: &str,
        destination: &str,
        title: &Option<EcoString>,
    ) -> WriteResult<()> {
        // Format: [label]: destination "optional title"
        self.write_char('[')?;
        self.write_str(label)?;
        self.write_str("]: ")?;
        self.write_str(destination)?;

        if let Some(title_text) = title {
            self.write_str(" \"")?;
            self.write_str(title_text)?;
            self.write_char('"')?;
        }

        Ok(())
    }

    /// Write a reference link
    pub(crate) fn write_reference_link(
        &mut self,
        label: &str,
        content: &[Node],
    ) -> WriteResult<()> {
        // Check for newlines in content
        for node in content {
            self.check_no_newline(node, "Reference Link Text")?;
        }

        // If content is empty or exactly matches the label (as plain text),
        // this is a shortcut reference link: [label]
        if content.is_empty() {
            self.write_char('[')?;
            self.write_str(label)?;
            self.write_char(']')?;
            return Ok(());
        }

        // Check if content is exactly the same as the label (to use shortcut syntax)
        let is_shortcut =
            content.len() == 1 && matches!(&content[0], Node::Text(text) if text == label);

        if is_shortcut {
            // Use shortcut reference link syntax: [label]
            self.write_char('[')?;
            self.write_str(label)?;
            self.write_char(']')?;
        } else {
            // Use full reference link syntax: [content][label]
            self.write_char('[')?;

            for node in content {
                self.write(node)?;
            }

            self.write_str("][")?;
            self.write_str(label)?;
            self.write_char(']')?;
        }

        Ok(())
    }

    /// Write an AST HtmlElement node as raw HTML string into the CommonMark output.
    pub(crate) fn write_html_element(
        &mut self,
        element: &crate::ast::HtmlElement,
    ) -> WriteResult<()> {
        if self.options.strict {
            if element.tag.contains('<') || element.tag.contains('>') {
                return Err(WriteError::InvalidHtmlTag(element.tag.clone()));
            }

            for attr in &element.attributes {
                if attr.name.contains('<') || attr.name.contains('>') {
                    return Err(WriteError::InvalidHtmlAttribute(attr.name.clone()));
                }
            }
        }

        use crate::writer::html::{HtmlWriter, HtmlWriterOptions};

        let html_options = if let Some(ref custom_options) = self.options.html_writer_options {
            custom_options.clone()
        } else {
            HtmlWriterOptions {
                strict: self.options.strict,
                code_block_language_class_prefix: Some("language-".into()),
                #[cfg(feature = "gfm")]
                enable_gfm: self.options.enable_gfm,
                #[cfg(feature = "gfm")]
                gfm_disallowed_html_tags: self.options.gfm_disallowed_html_tags.clone(),
            }
        };

        let mut html_writer = HtmlWriter::with_options(html_options);

        html_writer.write_node(&Node::HtmlElement(element.clone()))?;

        // Get the generated HTML
        let html_output = html_writer.into_string();

        // Otherwise write the raw HTML
        self.write_str(&html_output)
    }

    /// Get the generated CommonMark format text
    ///
    /// Consumes the writer and returns the generated string
    ///
    /// # Example
    ///
    /// ```
    /// use cmark_writer::writer::CommonMarkWriter;
    /// use cmark_writer::ast::Node;
    ///
    /// let mut writer = CommonMarkWriter::new();
    /// writer.write(&Node::Text("Hello".into())).unwrap();
    /// let result = writer.into_string();
    /// assert_eq!(result, "Hello");
    /// ```
    pub fn into_string(self) -> EcoString {
        self.buffer
    }

    /// Write a string to the output buffer
    ///
    /// This method is provided for custom node implementations to use
    pub fn write_str(&mut self, s: &str) -> WriteResult<()> {
        self.buffer.push_str(s);
        Ok(())
    }

    /// Write a character to the output buffer
    ///
    /// This method is provided for custom node implementations to use
    pub fn write_char(&mut self, c: char) -> WriteResult<()> {
        self.buffer.push(c);
        Ok(())
    }
    /// Ensure content ends with a newline (for consistent handling at the end of block nodes)
    ///
    /// Adds a newline character if the content doesn't already end with one; does nothing if it already ends with a newline
    pub(crate) fn ensure_trailing_newline(&mut self) -> WriteResult<()> {
        if !self.buffer.ends_with('\n') {
            self.write_char('\n')?;
        }
        Ok(())
    }

    /// Ensure there is a blank line (two consecutive newlines) at the end of the buffer
    pub(crate) fn ensure_blank_line(&mut self) -> WriteResult<()> {
        self.ensure_trailing_newline()?;
        if !self.buffer.ends_with("\n\n") {
            self.write_char('\n')?;
        }
        Ok(())
    }

    /// Write an emphasis (italic) node with custom delimiter
    pub(crate) fn write_emphasis(&mut self, content: &[Node]) -> WriteResult<()> {
        let delimiter = self.options.emphasis_char.to_string();
        self.write_delimited(content, &delimiter)
    }

    /// Write a strong emphasis (bold) node with custom delimiter
    pub(crate) fn write_strong(&mut self, content: &[Node]) -> WriteResult<()> {
        let char = self.options.strong_char;
        let delimiter = format!("{char}{char}");
        self.write_delimited(content, &delimiter)
    }

    /// Write a strikethrough node (GFM extension)
    #[cfg(feature = "gfm")]
    pub(crate) fn write_strikethrough(&mut self, content: &[Node]) -> WriteResult<()> {
        if !self.options.enable_gfm || !self.options.gfm_strikethrough {
            // If GFM strikethrough is disabled, just write the content without strikethrough
            for node in content.iter() {
                self.write(node)?;
            }
            return Ok(());
        }

        // Write content with ~~ delimiters
        self.write_delimited(content, "~~")
    }
}

impl Default for CommonMarkWriter {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Display trait for Node structure
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut writer = CommonMarkWriter::new();
        match writer.write(self) {
            Ok(_) => write!(f, "{}", writer.into_string()),
            Err(e) => write!(f, "Error writing Node: {e}"),
        }
    }
}

/// A trait for character escaping behavior
pub(crate) trait Escapes {
    /// Checks if the string needs escaping
    fn str_needs_escaping(s: &str) -> bool;

    /// Returns true if the character needs to be escaped
    fn char_needs_escaping(c: char) -> bool;

    /// Returns the escaped version of a character (if needed)
    fn escape_char(c: char) -> Option<&'static str>;
}

/// Markdown escaping implementation for CommonMark
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

/// A wrapper for efficient escaping
pub(crate) struct Escaped<'a, E: Escapes> {
    inner: &'a str,
    _phantom: std::marker::PhantomData<E>,
}

impl<'a, E: Escapes> Escaped<'a, E> {
    /// Create a new Escaped wrapper
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

impl CommonMarkWriter {
    /// Write a table as HTML (fallback for tables with block-level elements)
    fn write_table_as_html(&mut self, headers: &[Node], rows: &[Vec<Node>]) -> WriteResult<()> {
        use crate::writer::html::HtmlWriter;

        let mut html_writer = HtmlWriter::new();

        // Create table node for HTML writer
        let table_node = Node::Table {
            headers: headers.to_vec(),
            #[cfg(feature = "gfm")]
            alignments: vec![],
            rows: rows.to_vec(),
        };

        html_writer.write_node(&table_node).map_err(|_| {
            WriteError::HtmlFallbackError("Failed to write table as HTML".to_string().into())
        })?;

        let html_output = html_writer.into_string();
        self.buffer.push_str(&html_output);

        Ok(())
    }

    #[cfg(feature = "gfm")]
    /// Write a GFM table with alignment as HTML (fallback for tables with block-level elements)
    fn write_table_as_html_with_alignment(
        &mut self,
        headers: &[Node],
        alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        use crate::writer::html::HtmlWriter;

        let mut html_writer = HtmlWriter::new();

        // Create table node for HTML writer
        let table_node = Node::Table {
            headers: headers.to_vec(),
            alignments: alignments.to_vec(),
            rows: rows.to_vec(),
        };

        html_writer.write_node(&table_node).map_err(|_| {
            WriteError::HtmlFallbackError("Failed to write GFM table as HTML".to_string().into())
        })?;

        let html_output = html_writer.into_string();
        self.buffer.push_str(&html_output);

        Ok(())
    }
}

/// Escapes a string using the specified escaping strategy
pub(crate) fn escape_str<E: Escapes>(s: &str) -> std::borrow::Cow<'_, str> {
    if E::str_needs_escaping(s) {
        std::borrow::Cow::Owned(format!("{}", Escaped::<E>::new(s)))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(node: &Node) -> String {
        let mut writer = CommonMarkWriter::new();
        writer.write(node).unwrap();
        writer.into_string().into()
    }

    #[test]
    fn paragraphs_are_separated_by_blank_line() {
        let document = Node::Document(vec![
            Node::Paragraph(vec![Node::Text("First".into())]),
            Node::Paragraph(vec![Node::Text("Second".into())]),
        ]);

        assert_eq!(render(&document), "First\n\nSecond\n");
    }

    #[test]
    fn setext_heading_matches_content_width() {
        let heading = Node::Heading {
            level: 2,
            content: vec![Node::Text("Wide Title".into())],
            heading_type: HeadingType::Setext,
        };

        assert_eq!(render(&heading), "Wide Title\n----------\n");
    }

    #[test]
    fn autolink_preserves_url() {
        let autolink = Node::Autolink {
            url: "example.com/path".into(),
            is_email: false,
        };

        assert_eq!(render(&autolink), "<example.com/path>");
    }

    #[test]
    fn thematic_break_ends_with_newline() {
        let document = Node::Document(vec![
            Node::ThematicBreak,
            Node::Paragraph(vec![Node::Text("After".into())]),
        ]);

        assert_eq!(render(&document), "---\n\nAfter\n");
    }
}
