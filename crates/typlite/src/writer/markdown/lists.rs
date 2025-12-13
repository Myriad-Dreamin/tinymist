use crate::Result;
use crate::ir::{Block, Inline, ListItem};

use super::emit::IrMarkdownEmitter;
use super::escape::{append_to_last_line, indent_multiline};

impl IrMarkdownEmitter {
    pub(super) fn render_unordered_list(
        &mut self,
        items: &[ListItem],
        indent: usize,
    ) -> Result<String> {
        let mut out = String::new();
        for item in items {
            let ListItem::Unordered { content } = item else {
                continue;
            };
            self.render_list_item(&mut out, indent, "- ", content)?;
        }
        Ok(out.trim_end_matches('\n').to_string())
    }

    pub(super) fn render_ordered_list(
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
