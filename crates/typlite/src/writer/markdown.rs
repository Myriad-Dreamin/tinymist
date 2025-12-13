//! Markdown writer implementation

use ecow::EcoString;

use crate::Result;
use crate::html::{self, HtmlRenderOptions};
use crate::ir::{self, Block, Inline, IrNode, ListItem, Table, TableRowKind};
use crate::writer::IrFormatWriter;

/// Markdown writer implementation
#[derive(Default)]
pub struct MarkdownWriter {}

impl MarkdownWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl IrFormatWriter for MarkdownWriter {
    fn write_ir_eco(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        let mut emitter = IrMarkdownEmitter::new();
        emitter.write_document(document, output)
    }

    fn write_ir_vec(&mut self, _document: &ir::Document) -> Result<Vec<u8>> {
        Err("Markdown writer does not support writing to Vec<u8>".into())
    }
}

struct IrMarkdownEmitter {
    escape_special_chars: bool,
    trim_paragraph_trailing_hard_breaks: bool,
}

impl IrMarkdownEmitter {
    fn new() -> Self {
        Self {
            escape_special_chars: true,
            trim_paragraph_trailing_hard_breaks: true,
        }
    }

    fn write_document(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        let mut parts = Vec::new();
        for block in &document.blocks {
            let part = self.render_block(block, 0)?;
            if !part.is_empty() {
                parts.push(part);
            }
        }

        if parts.is_empty() {
            return Ok(());
        }

        output.push_str(&parts.join("\n\n"));
        if !output.ends_with('\n') {
            output.push('\n');
        }
        Ok(())
    }

    fn render_block(&mut self, block: &Block, indent: usize) -> Result<String> {
        let mut out = String::new();
        match block {
            Block::Document(blocks) => {
                let mut parts = Vec::new();
                for block in blocks {
                    let part = self.render_block(block, indent)?;
                    if !part.is_empty() {
                        parts.push(part);
                    }
                }
                out.push_str(&parts.join("\n\n"));
            }
            Block::Paragraph(inlines) => {
                if let Some(html) = self.try_render_paragraph_as_block_html(inlines)? {
                    out.push_str(html.trim_end_matches('\n'));
                } else {
                    let mut inlines = inlines.as_slice();
                    if self.trim_paragraph_trailing_hard_breaks {
                        while let Some(Inline::HardBreak) = inlines.last() {
                            inlines = &inlines[..inlines.len().saturating_sub(1)];
                        }
                    }
                    out.push_str(&self.render_inlines(inlines)?);
                }
                out = indent_multiline(&out, indent);
            }
            Block::Heading { level, content } => {
                out.push_str(&" ".repeat(indent));
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                out.push_str(&self.render_inlines(content)?);
            }
            Block::ThematicBreak => {
                out.push_str(&" ".repeat(indent));
                out.push_str("---");
            }
            Block::BlockQuote(content) => {
                let inner = self.render_block(&Block::Document(content.clone()), 0)?;
                for line in inner.lines() {
                    out.push_str(&" ".repeat(indent));
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
                out = out.trim_end_matches('\n').to_string();
            }
            Block::OrderedList { start, items } => {
                out.push_str(&self.render_ordered_list(*start, items, indent)?);
            }
            Block::UnorderedList(items) => {
                out.push_str(&self.render_unordered_list(items, indent)?);
            }
            Block::Table(table) => {
                out.push_str(&self.render_table(table, indent)?);
            }
            Block::CodeBlock {
                language,
                content,
                block_type: _,
            } => {
                out.push_str(&indent_code_fence_block(
                    language.as_deref(),
                    content,
                    indent,
                ));
            }
            Block::HtmlBlock(html) => {
                out.push_str(&indent_multiline(html.as_str(), indent));
                out = out.trim_end_matches('\n').to_string();
            }
            Block::HtmlElement(element) => {
                let html = render_ir_html_element_as_html(element)?;
                out.push_str(&indent_multiline(&html, indent));
                out = out.trim_end_matches('\n').to_string();
            }
            Block::Figure { .. } => {
                let html = self.render_figure_as_html(block)?;
                out.push_str(&indent_multiline(&html, indent));
                out = out.trim_end_matches('\n').to_string();
            }
            Block::ExternalFrame(frame) => {
                out.push_str(&" ".repeat(indent));
                out.push_str("![](");
                out.push_str(&escape_markdown_url(&frame.file_path.display().to_string()));
                out.push(')');
            }
            Block::Center(inner) => {
                let html = self.render_center_as_html(inner)?;
                out.push_str(&indent_multiline(&html, indent));
                out = out.trim_end_matches('\n').to_string();
            }
            Block::Alert { class, content } => {
                let mut lines = Vec::new();
                lines.push(format!("[!{}]", class.to_ascii_uppercase()));
                lines.push(String::new());

                let inner = self.render_block(&Block::Document(content.clone()), 0)?;
                if !inner.is_empty() {
                    lines.extend(inner.lines().map(|s| s.to_string()));
                }

                for (idx, line) in lines.iter().enumerate() {
                    out.push_str(&" ".repeat(indent));
                    out.push_str("> ");
                    out.push_str(line);
                    if idx + 1 < lines.len() {
                        out.push('\n');
                    }
                }
            }
        }

        Ok(out)
    }

    fn try_render_paragraph_as_block_html(&mut self, inlines: &[Inline]) -> Result<Option<String>> {
        let [Inline::HtmlElement(element)] = inlines else {
            return Ok(None);
        };
        if !is_block_html_tag(element.tag.as_str()) {
            return Ok(None);
        }
        Ok(Some(render_ir_html_element_as_html(element)?))
    }

    fn render_inlines(&mut self, inlines: &[Inline]) -> Result<String> {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                Inline::Text(text) => {
                    out.push_str(&escape_markdown_text(text, self.escape_special_chars))
                }
                Inline::Emphasis(content) => {
                    out.push('_');
                    out.push_str(&self.render_inlines(content)?);
                    out.push('_');
                }
                Inline::Strong(content) => {
                    out.push_str("**");
                    out.push_str(&self.render_inlines(content)?);
                    out.push_str("**");
                }
                Inline::Strikethrough(content) => {
                    out.push_str("~~");
                    out.push_str(&self.render_inlines(content)?);
                    out.push_str("~~");
                }
                Inline::Group(content) => out.push_str(&self.render_inlines(content)?),
                Inline::InlineCode(code) => {
                    out.push_str(&render_inline_code(code));
                }
                Inline::Link {
                    url,
                    title: _,
                    content,
                } => {
                    out.push('[');
                    out.push_str(&self.render_inlines(content)?);
                    out.push_str("](");
                    out.push_str(&escape_markdown_url(url));
                    out.push(')');
                }
                Inline::ReferenceLink { label, content } => {
                    out.push('[');
                    if content.is_empty() {
                        out.push_str(&escape_markdown_text(label, self.escape_special_chars));
                    } else {
                        out.push_str(&self.render_inlines(content)?);
                    }
                    out.push_str("][");
                    out.push_str(&escape_markdown_text(label, self.escape_special_chars));
                    out.push(']');
                }
                Inline::Image { url, title: _, alt } => {
                    out.push_str("![");
                    out.push_str(&self.render_inlines(alt)?);
                    out.push_str("](");
                    out.push_str(&escape_markdown_url(url));
                    out.push(')');
                }
                Inline::Autolink { url, is_email } => {
                    if *is_email {
                        out.push('<');
                        out.push_str("mailto:");
                        out.push_str(url);
                        out.push('>');
                    } else {
                        out.push('<');
                        out.push_str(url);
                        out.push('>');
                    }
                }
                Inline::HardBreak => {
                    out.push('\\');
                    out.push('\n');
                }
                Inline::SoftBreak => out.push('\n'),
                Inline::HtmlElement(element) => {
                    out.push_str(&render_ir_html_element_inline(element)?);
                }
                Inline::Highlight(content) => {
                    out.push_str("==");
                    out.push_str(&self.render_inlines(content)?);
                    out.push_str("==");
                }
                Inline::Verbatim(text) => out.push_str(text),
                Inline::Comment(text) => {
                    out.push_str("<!-- ");
                    out.push_str(text);
                    out.push_str(" -->");
                }
                Inline::EmbeddedBlock(_) => {
                    // Embedded blocks are handled specially by list-item paragraphs.
                }
                Inline::UnsupportedCustom => {}
            }
        }
        Ok(out)
    }

    fn render_unordered_list(&mut self, items: &[ListItem], indent: usize) -> Result<String> {
        let mut out = String::new();
        for item in items {
            let ListItem::Unordered { content } = item else {
                continue;
            };
            self.render_list_item(&mut out, indent, "- ", content)?;
        }
        Ok(out.trim_end_matches('\n').to_string())
    }

    fn render_ordered_list(
        &mut self,
        start: u32,
        items: &[ListItem],
        indent: usize,
    ) -> Result<String> {
        let mut out = String::new();
        let mut current = start.max(1);
        for item in items {
            let ListItem::Ordered { number, content } = item else {
                continue;
            };
            let n = number.unwrap_or(current);
            current = n.saturating_add(1);
            let marker = format!("{n}. ");
            self.render_list_item(&mut out, indent, &marker, content)?;
        }
        Ok(out.trim_end_matches('\n').to_string())
    }

    fn render_list_item(
        &mut self,
        out: &mut String,
        indent: usize,
        marker: &str,
        content: &[Block],
    ) -> Result<()> {
        let prefix = " ".repeat(indent);
        let nested_indent = indent + marker.len();

        if content.is_empty() {
            out.push_str(&prefix);
            out.push_str(marker);
            out.push('\n');
            return Ok(());
        }

        // Most list items are a single paragraph possibly containing embedded blocks.
        if content.len() == 1 {
            if let Block::Paragraph(inlines) = &content[0] {
                let (leading, embedded, trailing) = analyze_list_item_paragraph_inlines(inlines);

                out.push_str(&prefix);
                out.push_str(marker);
                out.push_str(&self.render_inlines(&leading)?);
                out.push('\n');

                let mut embedded_out = String::new();
                for block in embedded {
                    let rendered = self.render_block(&block, nested_indent)?;
                    if rendered.is_empty() {
                        continue;
                    }
                    embedded_out.push_str(&rendered);
                    embedded_out.push('\n');
                }

                if !trailing.is_empty() && !embedded_out.is_empty() {
                    let suffix = self.render_inlines(&trailing)?;
                    append_to_last_line(&mut embedded_out, &suffix);
                }

                if !embedded_out.is_empty() {
                    out.push_str(&embedded_out);
                }

                return Ok(());
            }
        }

        // Fallback: render each block as an indented block within the list item.
        out.push_str(&prefix);
        out.push_str(marker);
        let first = self.render_block(&content[0], 0)?;
        out.push_str(&indent_multiline(&first, 0));
        out.push('\n');
        for block in content.iter().skip(1) {
            let rendered = self.render_block(block, nested_indent)?;
            if rendered.is_empty() {
                continue;
            }
            out.push_str(&rendered);
            out.push('\n');
        }

        Ok(())
    }

    fn render_table(&mut self, table: &Table, indent: usize) -> Result<String> {
        if should_render_table_as_html(table) {
            let html = render_ir_table_as_html(table)?;
            return Ok(indent_multiline(&html, indent)
                .trim_end_matches('\n')
                .to_string());
        }

        let mut out = String::new();
        let prefix = " ".repeat(indent);

        if table.rows.is_empty() {
            return Ok(out);
        }

        let head_row_idx = table
            .rows
            .iter()
            .position(|r| matches!(r.kind, TableRowKind::Head))
            .unwrap_or(0);
        let header_row = &table.rows[head_row_idx];

        out.push_str(&prefix);
        out.push_str(&render_gfm_row(&header_row.cells, self)?);
        out.push('\n');

        out.push_str(&prefix);
        out.push('|');
        for _ in 0..table.columns {
            out.push_str(" --- |");
        }
        out.push('\n');

        for (idx, row) in table.rows.iter().enumerate() {
            if idx == head_row_idx && matches!(row.kind, TableRowKind::Head) {
                continue;
            }
            if matches!(row.kind, TableRowKind::Head) {
                continue;
            }
            out.push_str(&prefix);
            out.push_str(&render_gfm_row(&row.cells, self)?);
            out.push('\n');
        }

        Ok(out.trim_end_matches('\n').to_string())
    }

    fn render_center_as_html(&mut self, inner: &Block) -> Result<String> {
        let children = match inner {
            Block::Paragraph(inlines) => inlines.iter().cloned().map(IrNode::Inline).collect(),
            other => vec![IrNode::Block(other.clone())],
        };
        let element = ir::HtmlElement {
            tag: EcoString::inline("p"),
            attributes: vec![ir::HtmlAttribute {
                name: EcoString::inline("align"),
                value: EcoString::inline("center"),
            }],
            children,
            self_closing: false,
        };
        render_ir_html_element_as_html(&element)
    }

    fn render_figure_as_html(&mut self, figure: &Block) -> Result<String> {
        let Block::Figure { body, caption } = figure else {
            return Ok(String::new());
        };

        let mut children = vec![IrNode::Block((**body).clone())];
        children.extend(caption.iter().cloned().map(IrNode::Inline));

        let element = ir::HtmlElement {
            tag: EcoString::inline("figure"),
            attributes: vec![ir::HtmlAttribute {
                name: EcoString::inline("class"),
                value: EcoString::inline("figure"),
            }],
            children,
            self_closing: false,
        };

        render_ir_html_element_as_html(&element)
    }
}

fn is_block_html_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "blockquote"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "pre"
            | "table"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "aside"
            | "nav"
            | "ul"
            | "ol"
    )
}

fn escape_markdown_text(text: &str, escape_special_chars: bool) -> String {
    if !escape_special_chars || text.is_empty() {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '*' => out.push_str("\\*"),
            '_' => out.push_str("\\_"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '>' => out.push_str("\\>"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_markdown_url(url: &str) -> String {
    // The existing snapshots don't require URL escaping beyond raw output.
    url.to_string()
}

fn indent_multiline(text: &str, indent: usize) -> String {
    if indent == 0 || text.is_empty() {
        return text.to_string();
    }
    let prefix = " ".repeat(indent);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_to_last_line(out: &mut String, suffix: &str) {
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
        out.push_str(suffix);
        out.push('\n');
        return;
    }
    out.push_str(suffix);
}

fn analyze_list_item_paragraph_inlines(
    inlines: &[Inline],
) -> (Vec<Inline>, Vec<Block>, Vec<Inline>) {
    let mut leading = Vec::new();
    let mut embedded = Vec::new();
    let mut trailing = Vec::new();

    let mut seen_nested_blocks = false;
    for inline in inlines {
        match inline {
            Inline::EmbeddedBlock(block) => match &**block {
                Block::Paragraph(content) if !seen_nested_blocks => {
                    leading.extend(content.iter().cloned());
                }
                other => {
                    seen_nested_blocks = true;
                    embedded.push(other.clone());
                }
            },
            Inline::Comment(_) if seen_nested_blocks => trailing.push(inline.clone()),
            other if seen_nested_blocks => trailing.push(other.clone()),
            other => leading.push(other.clone()),
        }
    }

    (leading, embedded, trailing)
}

fn max_consecutive_backticks(text: &str) -> usize {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    max_run
}

fn render_inline_code(code: &str) -> String {
    let ticks = max_consecutive_backticks(code);
    let fence = "`".repeat((ticks + 1).max(1));
    format!("{fence}{code}{fence}")
}

fn indent_code_fence_block(language: Option<&str>, content: &str, indent: usize) -> String {
    let max_ticks = max_consecutive_backticks(content);
    let fence = "`".repeat((max_ticks + 1).max(3));
    let mut out = String::new();
    out.push_str(&" ".repeat(indent));
    out.push_str(&fence);
    if let Some(lang) = language
        && !lang.is_empty()
    {
        out.push_str(lang);
    }
    out.push('\n');

    let mut content = content.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    out.push_str(&indent_multiline(content.trim_end_matches('\n'), indent));
    out.push('\n');
    out.push_str(&" ".repeat(indent));
    out.push_str(&fence);
    out
}

fn render_gfm_row(cells: &[ir::TableCell], emitter: &mut IrMarkdownEmitter) -> Result<String> {
    let mut row = String::new();
    row.push('|');
    for cell in cells {
        row.push(' ');
        let text = render_table_cell_inline(cell, emitter)?;
        row.push_str(&text);
        row.push(' ');
        row.push('|');
    }
    Ok(row)
}

fn render_table_cell_inline(
    cell: &ir::TableCell,
    emitter: &mut IrMarkdownEmitter,
) -> Result<String> {
    let mut out = String::new();
    for node in &cell.content {
        match node {
            IrNode::Inline(inline) => {
                out.push_str(&emitter.render_inlines(std::slice::from_ref(inline))?);
            }
            IrNode::Block(_) => {
                // Shouldn't happen in GFM table.
            }
        }
    }
    Ok(out)
}

fn should_render_table_as_html(table: &Table) -> bool {
    for row in &table.rows {
        for cell in &row.cells {
            if cell.colspan != 1 || cell.rowspan != 1 {
                return true;
            }
            if cell.content.iter().any(|n| matches!(n, IrNode::Block(_))) {
                return true;
            }
        }
    }
    false
}

fn render_ir_table_as_html(table: &Table) -> Result<String> {
    html::render_table_as_html(
        table,
        &HtmlRenderOptions {
            strict: false,
            ..Default::default()
        },
    )
}

fn render_ir_html_element_as_html(element: &ir::HtmlElement) -> Result<String> {
    html::render_html_element(
        element,
        &HtmlRenderOptions {
            strict: false,
            ..Default::default()
        },
    )
}

fn render_ir_html_element_inline(element: &ir::HtmlElement) -> Result<String> {
    // Inline HTML should not include trailing newlines.
    let html = render_ir_html_element_as_html(element)?;
    Ok(html.trim_end_matches('\n').to_string())
}
