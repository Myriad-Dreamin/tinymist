use crate::ast::{CodeBlockType, HeadingType, ListItem, Node};
#[cfg(feature = "gfm")]
use crate::ast::{TableAlignment, TaskListStatus};
use crate::error::{WriteError, WriteResult};
use ecow::EcoString;

use super::super::utils::table_contains_block_elements;
use super::super::CommonMarkWriter;

impl CommonMarkWriter {
    /// Apply a prefix to multi-line content, preserving the first-line prefix if provided.
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

        if let Some(first) = lines.first() {
            let actual_prefix = first_line_prefix.unwrap_or(prefix);
            result.push_str(actual_prefix);
            result.push_str(first);
        }

        for line in &lines[1..] {
            result.push('\n');
            result.push_str(prefix);
            result.push_str(line);
        }

        result
    }

    /// Write a document node and insert appropriate spacing between blocks.
    pub(crate) fn write_document(&mut self, children: &[Node]) -> WriteResult<()> {
        let mut prev_was_block = false;

        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                self.prepare_block_sequence(prev_was_block, child.is_block())?;
            }

            self.write(child)?;

            if child.is_block() {
                self.ensure_trailing_newline()?;
            }

            prev_was_block = child.is_block();
        }

        Ok(())
    }

    /// Write a heading node, normalising invalid levels when non-strict.
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
                let original = level;
                level = level.clamp(1, 6);
                self.emit_warning(format!(
                    "Invalid heading level: {original}. Corrected to {level}."
                ));
            }
        }

        match heading_type {
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
                let heading_text = self.capture_with_buffer(|writer| {
                    for node in content {
                        writer.write(node)?;
                    }
                    Ok(())
                })?;

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

    /// Write a paragraph node with optional trimming of trailing hard breaks.
    pub(crate) fn write_paragraph(&mut self, content: &[Node]) -> WriteResult<()> {
        if self.options.trim_paragraph_trailing_hard_breaks {
            let mut last_non_hard_break = content.len();

            while last_non_hard_break > 0 {
                if !matches!(content[last_non_hard_break - 1], Node::HardBreak) {
                    break;
                }
                last_non_hard_break -= 1;
            }

            for node in content.iter().take(last_non_hard_break) {
                self.write(node)?;
            }
        } else {
            for node in content {
                self.write(node)?;
            }
        }

        self.ensure_trailing_newline()
    }

    /// Write a blockquote, indenting inner content with the `> ` prefix.
    pub(crate) fn write_blockquote(&mut self, content: &[Node]) -> WriteResult<()> {
        let all_content = self.capture_with_buffer(|writer| {
            writer.write_document(content)?;
            Ok(())
        })?;

        let prefix = "> ";
        let formatted_content = self.apply_prefix(&all_content, prefix, Some(prefix));
        self.buffer.push_str(&formatted_content);
        Ok(())
    }

    /// Write a thematic break (horizontal rule).
    pub(crate) fn write_thematic_break(&mut self) -> WriteResult<()> {
        let ch = self.options.thematic_break_char;
        self.write_str(&format!("{ch}{ch}{ch}"))?;
        self.ensure_trailing_newline()
    }

    /// Write a code block, supporting indented and fenced styles.
    pub(crate) fn write_code_block(
        &mut self,
        language: &Option<EcoString>,
        content: &EcoString,
        block_type: &CodeBlockType,
    ) -> WriteResult<()> {
        let content = content.as_ref();
        match block_type {
            CodeBlockType::Indented => {
                let indent = "    ";
                let indented = self.apply_prefix(content, indent, Some(indent));
                self.buffer.push_str(&indented);
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

    /// Write an unordered list using the configured list marker.
    pub(crate) fn write_unordered_list(&mut self, items: &[ListItem]) -> WriteResult<()> {
        let list_marker = self.options.list_marker;
        let prefix = format!("{list_marker} ");

        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.write_char('\n')?;
            }
            self.write_list_item(item, &prefix)?;
        }

        Ok(())
    }

    /// Write an ordered list, respecting explicit numbering overrides.
    pub(crate) fn write_ordered_list(&mut self, start: u32, items: &[ListItem]) -> WriteResult<()> {
        let mut current_number = start;

        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.write_char('\n')?;
            }

            match item {
                ListItem::Ordered {
                    number: Some(custom),
                    ..
                } => {
                    let prefix = format!("{custom}. ");
                    self.write_list_item(item, &prefix)?;
                    current_number = custom + 1;
                }
                _ => {
                    let prefix = format!("{current_number}. ");
                    self.write_list_item(item, &prefix)?;
                    current_number += 1;
                }
            }
        }

        Ok(())
    }

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
                if self.options.gfm_tasklists {
                    let checkbox = match status {
                        TaskListStatus::Checked => "[x] ",
                        TaskListStatus::Unchecked => "[ ] ",
                    };

                    let task_prefix = format!("{prefix}{checkbox}");
                    self.write_str(&task_prefix)?;
                    self.write_list_item_content(content, task_prefix.len())?;
                } else {
                    self.write_str(prefix)?;
                    self.write_list_item_content(content, prefix.len())?;
                }
            }
        }

        Ok(())
    }

    fn write_list_item_content(&mut self, content: &[Node], prefix_len: usize) -> WriteResult<()> {
        if content.is_empty() {
            return Ok(());
        }

        let all_content = self.capture_with_buffer(|writer| {
            writer.write_document(content)?;
            Ok(())
        })?;

        let indent = " ".repeat(prefix_len);
        let formatted = self.apply_prefix(&all_content, &indent, Some(""));
        self.buffer.push_str(&formatted);
        Ok(())
    }

    /// Write a table, falling back to HTML when block elements appear in cells.
    pub(crate) fn write_table(&mut self, headers: &[Node], rows: &[Vec<Node>]) -> WriteResult<()> {
        if table_contains_block_elements(headers, rows) {
            if self.is_strict_mode() {
                return Err(WriteError::InvalidStructure(
                    "Table contains block-level elements which are not allowed in strict mode"
                        .to_string()
                        .into(),
                ));
            } else {
                self.emit_info(
                    "Table contains block-level elements, falling back to HTML output in soft mode",
                );
                return self.write_table_as_html(headers, rows);
            }
        }

        self.write_char('|')?;
        for header in headers {
            self.check_no_newline(header, "Table Header")?;
            self.write_char(' ')?;
            self.write(header)?;
            self.write_str(" |")?;
        }
        self.write_char('\n')?;

        self.write_char('|')?;
        for _ in 0..headers.len() {
            self.write_str(" --- |")?;
        }
        self.write_char('\n')?;

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

    /// Write a GFM table with per-column alignment when enabled.
    #[cfg(feature = "gfm")]
    pub(crate) fn write_table_with_alignment(
        &mut self,
        headers: &[Node],
        alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> WriteResult<()> {
        if !self.options.gfm_tables {
            return self.write_table(headers, rows);
        }

        if table_contains_block_elements(headers, rows) {
            if self.is_strict_mode() {
                return Err(WriteError::InvalidStructure(
                    "GFM table contains block-level elements which are not allowed in strict mode"
                        .to_string()
                        .into(),
                ));
            } else {
                self.emit_info(
                        "GFM table contains block-level elements, falling back to HTML output in soft mode",
                    );
                return self.write_table_as_html_with_alignment(headers, alignments, rows);
            }
        }

        self.write_char('|')?;
        for header in headers {
            self.check_no_newline(header, "Table Header")?;
            self.write_char(' ')?;
            self.write(header)?;
            self.write_str(" |")?;
        }
        self.write_char('\n')?;

        self.write_char('|')?;
        for index in 0..headers.len() {
            let alignment = alignments.get(index).unwrap_or(&TableAlignment::Center);
            match alignment {
                TableAlignment::Left => self.write_str(" :--- |")?,
                TableAlignment::Center => self.write_str(" :---: |")?,
                TableAlignment::Right => self.write_str(" ---: |")?,
                TableAlignment::None => self.write_str(" --- |")?,
            }
        }
        self.write_char('\n')?;

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

    /// Write an HTML block verbatim.
    pub(crate) fn write_html_block(&mut self, content: &EcoString) -> WriteResult<()> {
        self.buffer.push_str(content.as_ref());
        Ok(())
    }
}
