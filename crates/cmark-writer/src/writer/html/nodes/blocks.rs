use ecow::EcoString;

use super::super::core::GuardedHtmlElement;
use super::super::{HtmlWriteResult, HtmlWriter};
#[cfg(feature = "gfm")]
use crate::ast::TaskListStatus;
use crate::ast::{
    HtmlElement, ListItem, Node, TableAlignment, TableCellKind, TableRow, TableRowKind,
};

impl HtmlWriter {
    pub(crate) fn write_document(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        for child in children {
            self.write_node(child)?;
            if child.is_block() && !self.buffer.ends_with('\n') {
                // Keep HTML output compact; intentionally skip inserting extra
                // newlines by default.
            }
        }
        Ok(())
    }

    pub(crate) fn write_paragraph(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag("p")?;
        self.finish_tag()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag("p")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_heading(&mut self, level: u8, content: &[Node]) -> HtmlWriteResult<()> {
        let tag_name = format!("h{}", level.clamp(1, 6));
        self.start_tag(&tag_name)?;
        self.finish_tag()?;
        for child in content {
            self.write_node(child)?;
        }
        self.end_tag(&tag_name)?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_thematic_break(&mut self) -> HtmlWriteResult<()> {
        self.self_closing_tag("hr")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_code_block(
        &mut self,
        language: &Option<EcoString>,
        content: &str,
    ) -> HtmlWriteResult<()> {
        self.start_tag("pre")?;
        self.finish_tag()?;
        self.start_tag("code")?;
        if let Some(prefix) = &self.options.code_block_language_class_prefix {
            if let Some(lang) = language {
                if !lang.is_empty() {
                    self.attribute("class", &format!("{}{}", prefix, lang.trim()))?;
                }
            }
        }
        self.finish_tag()?;
        self.text(content)?;
        self.end_tag("code")?;
        self.end_tag("pre")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_html_block(&mut self, block_content: &str) -> HtmlWriteResult<()> {
        self.write_trusted_html(block_content)?;
        if !block_content.ends_with('\n') {
            self.write_trusted_html("\n")?;
        }
        Ok(())
    }

    pub(crate) fn write_html_element(&mut self, element: &HtmlElement) -> HtmlWriteResult<()> {
        match self.guard_html_element(element)? {
            GuardedHtmlElement::Textualize => self.textualize_html_element(element),
            GuardedHtmlElement::Render(mut guard) => {
                guard.write_attributes(&element.attributes)?;
                if element.self_closing {
                    guard.finish_self_closing()?;
                    return Ok(());
                }

                let mut body = guard.finish()?;
                body.writer().buffer.push_str("\n\n");
                for child in &element.children {
                    body.writer().write_node(child)?;
                }
                body.writer().buffer.push_str("\n\n");
                body.end()?;
                Ok(())
            }
        }
    }

    pub(crate) fn textualize_html_element(&mut self, element: &HtmlElement) -> HtmlWriteResult<()> {
        self.text("<")?;
        self.text(&element.tag)?;
        for attr in &element.attributes {
            self.text(" ")?;
            self.text(&attr.name)?;
            self.text("=")?;
            self.text("\"")?;
            self.text(&attr.value)?;
            self.text("\"")?;
        }
        if element.self_closing {
            self.text(" />")?;
        } else {
            self.text(">")?;
            self.buffer.push_str("\n\n");
            for child in &element.children {
                self.write_node(child)?;
            }
            self.buffer.push_str("\n\n");
            self.text("</")?;
            self.text(&element.tag)?;
            self.text(">")?;
        }
        Ok(())
    }

    pub(crate) fn write_blockquote(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag("blockquote")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag("blockquote")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    fn write_list_item_content(&mut self, item_content: &[Node]) -> HtmlWriteResult<()> {
        let mut add_newline_before_next_child = false;
        for child_node in item_content.iter() {
            if add_newline_before_next_child {
                self.write_trusted_html("\n")?;
                add_newline_before_next_child = false;
            }
            self.write_node(child_node)?;
            if child_node.is_block() {
                add_newline_before_next_child = true;
            }
        }
        Ok(())
    }

    pub(crate) fn write_list_item(&mut self, item: &ListItem) -> HtmlWriteResult<()> {
        self.start_tag("li")?;

        #[cfg(feature = "gfm")]
        if self.options.enable_gfm {
            if let ListItem::Task { status, .. } = item {
                let class_name = if *status == TaskListStatus::Checked {
                    "task-list-item task-list-item-checked"
                } else {
                    "task-list-item"
                };
                self.attribute("class", class_name)?;
            }
        }
        self.finish_tag()?;

        let content = match item {
            ListItem::Unordered { content } => content,
            ListItem::Ordered { content, .. } => content,
            #[cfg(feature = "gfm")]
            ListItem::Task { content, .. } => content,
        };

        #[cfg(feature = "gfm")]
        if self.options.enable_gfm {
            if let ListItem::Task { status, .. } = item {
                self.start_tag("input")?;
                self.attribute("type", "checkbox")?;
                self.attribute("disabled", "")?;
                if *status == TaskListStatus::Checked {
                    self.attribute("checked", "")?;
                }
                self.finish_self_closing_tag()?;
                self.write_trusted_html(" ")?;
            }
        }

        self.write_list_item_content(content)?;
        self.end_tag("li")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_ordered_list(
        &mut self,
        start: u32,
        items: &[ListItem],
    ) -> HtmlWriteResult<()> {
        self.start_tag("ol")?;
        if start != 1 {
            self.attribute("start", &start.to_string())?;
        }
        self.finish_tag()?;
        self.write_trusted_html("\n")?;
        for item in items {
            self.write_list_item(item)?;
        }
        self.end_tag("ol")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_unordered_list(&mut self, items: &[ListItem]) -> HtmlWriteResult<()> {
        self.start_tag("ul")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;
        for item in items {
            self.write_list_item(item)?;
        }
        self.end_tag("ul")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    pub(crate) fn write_table(
        &mut self,
        _columns: usize,
        rows: &[TableRow],
        alignments: &[TableAlignment],
    ) -> HtmlWriteResult<()> {
        self.render_table(rows, alignments)
    }

    fn render_table(
        &mut self,
        rows: &[TableRow],
        alignments: &[TableAlignment],
    ) -> HtmlWriteResult<()> {
        self.start_tag("table")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;

        let (head_rows, body_rows, foot_rows) = Self::partition_rows(rows);

        if !head_rows.is_empty() {
            self.start_tag("thead")?;
            self.finish_tag()?;
            self.write_trusted_html("\n")?;
            for row in head_rows {
                self.write_table_row(row, alignments)?;
            }
            self.end_tag("thead")?;
            self.write_trusted_html("\n")?;
        }

        self.start_tag("tbody")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;
        for row in body_rows {
            self.write_table_row(row, alignments)?;
        }
        self.end_tag("tbody")?;
        self.write_trusted_html("\n")?;

        if !foot_rows.is_empty() {
            self.start_tag("tfoot")?;
            self.finish_tag()?;
            self.write_trusted_html("\n")?;
            for row in foot_rows {
                self.write_table_row(row, alignments)?;
            }
            self.end_tag("tfoot")?;
            self.write_trusted_html("\n")?;
        }

        self.end_tag("table")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    fn partition_rows(
        rows: &[TableRow],
    ) -> (&[TableRow], &[TableRow], &[TableRow]) {
        let mut head_end = 0;
        while head_end < rows.len() && matches!(rows[head_end].kind, TableRowKind::Head) {
            head_end += 1;
        }

        let mut foot_start = rows.len();
        while foot_start > head_end && matches!(rows[foot_start - 1].kind, TableRowKind::Foot) {
            foot_start -= 1;
        }

        let head = &rows[..head_end];
        let body = &rows[head_end..foot_start];
        let foot = &rows[foot_start..];
        (head, body, foot)
    }

    fn write_table_row(
        &mut self,
        row: &TableRow,
        alignments: &[TableAlignment],
    ) -> HtmlWriteResult<()> {
        self.start_tag("tr")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;

        for (col_index, cell) in row.cells.iter().enumerate() {
            let tag = match cell.kind {
                TableCellKind::Header => "th",
                TableCellKind::Data => "td",
            };
            self.start_tag(tag)?;
            if cell.colspan > 1 {
                self.attribute("colspan", &cell.colspan.to_string())?;
            }
            if cell.rowspan > 1 {
                self.attribute("rowspan", &cell.rowspan.to_string())?;
            }
            if let Some(effective) = cell.align.as_ref().or_else(|| alignments.get(col_index)) {
                match effective {
                    TableAlignment::Left => self.attribute("style", "text-align: left;")?,
                    TableAlignment::Center => self.attribute("style", "text-align: center;")?,
                    TableAlignment::Right => self.attribute("style", "text-align: right;")?,
                    TableAlignment::None => {}
                }
            }
            self.finish_tag()?;
            self.buffer.push_str("\n\n");
            self.write_node(&cell.content)?;
            self.buffer.push_str("\n\n");
            self.end_tag(tag)?;
            self.write_trusted_html("\n")?;
        }

        self.end_tag("tr")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }
}
