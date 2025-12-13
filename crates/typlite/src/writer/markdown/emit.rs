use ecow::EcoString;

use crate::Result;
use crate::ir::{self, Block, Inline, IrNode};

use super::escape::{
    escape_markdown_text, escape_markdown_url, indent_code_fence_block, indent_multiline,
    render_inline_code,
};
use super::render_html::{render_ir_html_element_as_html, render_ir_html_element_inline};

pub(super) struct IrMarkdownEmitter {
    escape_special_chars: bool,
    trim_paragraph_trailing_hard_breaks: bool,
}

impl IrMarkdownEmitter {
    pub(super) fn new() -> Self {
        Self {
            escape_special_chars: true,
            trim_paragraph_trailing_hard_breaks: true,
        }
    }

    pub(super) fn write_document(
        &mut self,
        document: &ir::Document,
        output: &mut EcoString,
    ) -> Result<()> {
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

    pub(super) fn render_block(&mut self, block: &Block, indent: usize) -> Result<String> {
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

    pub(super) fn render_inlines(&mut self, inlines: &[Inline]) -> Result<String> {
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
                Inline::InlineCode(code) => out.push_str(&render_inline_code(code)),
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
                    out.push('<');
                    if *is_email {
                        out.push_str("mailto:");
                    }
                    out.push_str(url);
                    out.push('>');
                }
                Inline::HardBreak => {
                    out.push('\\');
                    out.push('\n');
                }
                Inline::SoftBreak => out.push('\n'),
                Inline::HtmlElement(element) => {
                    out.push_str(&render_ir_html_element_inline(element)?)
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
