use ecow::EcoString;

use crate::Result;
use crate::ir::{self, Block, Inline, IrNode, ListItem, Table, TableCellKind, TableRowKind};

#[derive(Debug, Clone)]
pub(crate) struct HtmlRenderOptions {
    pub strict: bool,
    pub code_block_language_class_prefix: Option<EcoString>,
}

impl Default for HtmlRenderOptions {
    fn default() -> Self {
        Self {
            strict: true,
            code_block_language_class_prefix: Some("language-".into()),
        }
    }
}

pub(crate) fn render_html_element(
    element: &ir::HtmlElement,
    options: &HtmlRenderOptions,
) -> Result<String> {
    let mut renderer = IrHtmlRenderer::new(options.clone());
    renderer.write_html_element(element)?;
    renderer.into_string()
}

pub(crate) fn render_table_as_html(table: &Table, options: &HtmlRenderOptions) -> Result<String> {
    let mut renderer = IrHtmlRenderer::new(options.clone());
    renderer.write_table(table)?;
    renderer.into_string()
}

struct IrHtmlRenderer {
    options: HtmlRenderOptions,
    buffer: String,
    tag_opened: bool,
}

impl IrHtmlRenderer {
    fn new(options: HtmlRenderOptions) -> Self {
        Self {
            options,
            buffer: String::new(),
            tag_opened: false,
        }
    }

    fn into_string(mut self) -> Result<String> {
        self.ensure_tag_closed()?;
        Ok(self.buffer)
    }

    fn ensure_tag_closed(&mut self) -> Result<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    fn start_tag(&mut self, tag_name: &str) -> Result<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.tag_opened = true;
        Ok(())
    }

    fn attribute(&mut self, key: &str, value: &str) -> Result<()> {
        if !self.tag_opened {
            return Err("Cannot write attribute: no tag is currently open.".into());
        }
        self.buffer.push(' ');
        self.buffer.push_str(key);
        self.buffer.push_str("=\"");
        self.buffer
            .push_str(html_escape::encode_double_quoted_attribute(value).as_ref());
        self.buffer.push('"');
        Ok(())
    }

    fn finish_tag(&mut self) -> Result<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    fn finish_self_closing_tag(&mut self) -> Result<()> {
        if !self.tag_opened {
            return Err("Cannot finish self-closing tag: no tag is currently open.".into());
        }
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    fn self_closing_tag(&mut self, tag_name: &str) -> Result<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    fn end_tag(&mut self, tag_name: &str) -> Result<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str("</");
        self.buffer.push_str(tag_name);
        self.buffer.push('>');
        Ok(())
    }

    fn text(&mut self, text: &str) -> Result<()> {
        self.ensure_tag_closed()?;
        self.buffer
            .push_str(html_escape::encode_text(text).as_ref());
        Ok(())
    }

    fn write_trusted_html(&mut self, html: &str) -> Result<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str(html);
        Ok(())
    }

    fn is_safe_tag_name(tag: &str) -> bool {
        !tag.is_empty()
            && tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
    }

    fn is_safe_attribute_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-' || c == '.')
    }

    fn write_node(&mut self, node: &IrNode) -> Result<()> {
        match node {
            IrNode::Block(block) => self.write_block(block),
            IrNode::Inline(inline) => self.write_inline(inline),
        }
    }

    fn write_document(&mut self, blocks: &[Block]) -> Result<()> {
        for block in blocks {
            self.write_block(block)?;
        }
        Ok(())
    }

    fn write_block(&mut self, block: &Block) -> Result<()> {
        match block {
            Block::Document(blocks) => self.write_document(blocks),
            Block::Paragraph(inlines) => {
                self.start_tag("p")?;
                self.finish_tag()?;
                for inline in inlines {
                    self.write_inline(inline)?;
                }
                self.end_tag("p")?;
                self.write_trusted_html("\n")?;
                Ok(())
            }
            Block::Heading { level, content } => {
                let tag_name = format!("h{}", (*level).clamp(1, 6));
                self.start_tag(&tag_name)?;
                self.finish_tag()?;
                for inline in content {
                    self.write_inline(inline)?;
                }
                self.end_tag(&tag_name)?;
                self.write_trusted_html("\n")?;
                Ok(())
            }
            Block::ThematicBreak => {
                self.self_closing_tag("hr")?;
                self.write_trusted_html("\n")?;
                Ok(())
            }
            Block::CodeBlock {
                language, content, ..
            } => {
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
            Block::HtmlBlock(html) => {
                self.write_trusted_html(html)?;
                if !html.ends_with('\n') {
                    self.write_trusted_html("\n")?;
                }
                Ok(())
            }
            Block::HtmlElement(element) => self.write_html_element(element),
            Block::BlockQuote(content) => {
                self.start_tag("blockquote")?;
                self.finish_tag()?;
                self.write_trusted_html("\n")?;
                for child in content {
                    self.write_block(child)?;
                }
                self.end_tag("blockquote")?;
                self.write_trusted_html("\n")?;
                Ok(())
            }
            Block::OrderedList { start, items } => {
                self.start_tag("ol")?;
                if *start != 1 {
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
            Block::UnorderedList(items) => {
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
            Block::Table(table) => self.write_table(table),
            Block::Figure { body, caption } => {
                let element = ir::HtmlElement {
                    tag: EcoString::inline("figure"),
                    attributes: vec![ir::HtmlAttribute {
                        name: EcoString::inline("class"),
                        value: EcoString::inline("figure"),
                    }],
                    children: {
                        let mut children = vec![IrNode::Block((**body).clone())];
                        children.extend(caption.iter().cloned().map(IrNode::Inline));
                        children
                    },
                    self_closing: false,
                };
                self.write_html_element(&element)
            }
            Block::ExternalFrame(frame) => {
                let element = ir::HtmlElement {
                    tag: EcoString::inline("img"),
                    attributes: vec![
                        ir::HtmlAttribute {
                            name: EcoString::inline("src"),
                            value: frame.file_path.display().to_string().into(),
                        },
                        ir::HtmlAttribute {
                            name: EcoString::inline("alt"),
                            value: frame.alt_text.clone(),
                        },
                    ],
                    children: vec![],
                    self_closing: true,
                };
                self.write_html_element(&element)
            }
            Block::Center(inner) => {
                let children = match &**inner {
                    Block::Paragraph(inlines) => {
                        inlines.iter().cloned().map(IrNode::Inline).collect()
                    }
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
                self.write_html_element(&element)
            }
            Block::Alert { class, content } => {
                // Preserve existing behavior of emitting alerts as blockquotes in HTML rendering.
                let mut inner = Vec::new();
                inner.push(Block::Paragraph(vec![Inline::Text(
                    format!("[!{}]", class.to_ascii_uppercase()).into(),
                )]));
                inner.push(Block::Paragraph(vec![Inline::Text("".into())]));
                inner.extend(content.clone());
                self.write_block(&Block::BlockQuote(inner))
            }
        }
    }

    fn write_list_item_content(&mut self, blocks: &[Block]) -> Result<()> {
        let mut add_newline_before_next_child = false;
        for block in blocks {
            if add_newline_before_next_child {
                self.write_trusted_html("\n")?;
            }
            self.write_block(block)?;
            add_newline_before_next_child = true;
        }
        Ok(())
    }

    fn write_list_item(&mut self, item: &ListItem) -> Result<()> {
        self.start_tag("li")?;
        self.finish_tag()?;

        let content = match item {
            ListItem::Unordered { content } => content,
            ListItem::Ordered { content, .. } => content,
        };

        self.write_list_item_content(content)?;
        self.end_tag("li")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    fn write_table(&mut self, table: &Table) -> Result<()> {
        self.start_tag("table")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;

        let (head_rows, body_rows, foot_rows) = Self::partition_rows(&table.rows);

        if !head_rows.is_empty() {
            self.start_tag("thead")?;
            self.finish_tag()?;
            self.write_trusted_html("\n")?;
            for row in head_rows {
                self.write_table_row(row)?;
            }
            self.end_tag("thead")?;
            self.write_trusted_html("\n")?;
        }

        self.start_tag("tbody")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;
        for row in body_rows {
            self.write_table_row(row)?;
        }
        self.end_tag("tbody")?;
        self.write_trusted_html("\n")?;

        if !foot_rows.is_empty() {
            self.start_tag("tfoot")?;
            self.finish_tag()?;
            self.write_trusted_html("\n")?;
            for row in foot_rows {
                self.write_table_row(row)?;
            }
            self.end_tag("tfoot")?;
            self.write_trusted_html("\n")?;
        }

        self.end_tag("table")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    fn partition_rows(
        rows: &[ir::TableRow],
    ) -> (&[ir::TableRow], &[ir::TableRow], &[ir::TableRow]) {
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

    fn write_table_row(&mut self, row: &ir::TableRow) -> Result<()> {
        self.start_tag("tr")?;
        self.finish_tag()?;
        self.write_trusted_html("\n")?;

        for cell in &row.cells {
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
            self.finish_tag()?;

            self.buffer.push_str("\n\n");
            if cell.content.is_empty() {
                // Matches the cmark-writer behavior of emitting an empty text node.
            } else {
                for node in &cell.content {
                    self.write_node(node)?;
                }
            }
            self.buffer.push_str("\n\n");

            self.end_tag(tag)?;
            self.write_trusted_html("\n")?;
        }

        self.end_tag("tr")?;
        self.write_trusted_html("\n")?;
        Ok(())
    }

    fn write_html_element(&mut self, element: &ir::HtmlElement) -> Result<()> {
        let tag = element.tag.as_str();

        if !Self::is_safe_tag_name(tag) {
            if self.options.strict {
                return Err(format!("Invalid HTML tag name: {tag}").into());
            }
            return self.textualize_html_element(element);
        }

        for attr in &element.attributes {
            if !Self::is_safe_attribute_name(attr.name.as_str()) {
                if self.options.strict {
                    return Err(format!("Invalid HTML attribute name: {}", attr.name).into());
                }
                return self.textualize_html_element(element);
            }
        }

        self.start_tag(tag)?;
        for attr in &element.attributes {
            self.attribute(attr.name.as_str(), attr.value.as_str())?;
        }

        if element.self_closing {
            self.finish_self_closing_tag()?;
            return Ok(());
        }

        self.finish_tag()?;
        self.buffer.push_str("\n\n");
        for child in &element.children {
            self.write_node(child)?;
        }
        self.buffer.push_str("\n\n");
        self.end_tag(tag)?;
        Ok(())
    }

    fn textualize_html_element(&mut self, element: &ir::HtmlElement) -> Result<()> {
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
            return Ok(());
        }

        self.text(">")?;
        self.buffer.push_str("\n\n");
        for child in &element.children {
            self.write_node(child)?;
        }
        self.buffer.push_str("\n\n");
        self.text("</")?;
        self.text(&element.tag)?;
        self.text(">")?;
        Ok(())
    }

    fn write_inline(&mut self, inline: &Inline) -> Result<()> {
        match inline {
            Inline::Text(text) => self.text(text),
            Inline::Emphasis(content) => {
                self.start_tag("em")?;
                self.finish_tag()?;
                for child in content {
                    self.write_inline(child)?;
                }
                self.end_tag("em")?;
                Ok(())
            }
            Inline::Strong(content) => {
                self.start_tag("strong")?;
                self.finish_tag()?;
                for child in content {
                    self.write_inline(child)?;
                }
                self.end_tag("strong")?;
                Ok(())
            }
            Inline::Strikethrough(content) => {
                self.start_tag("del")?;
                self.finish_tag()?;
                for child in content {
                    self.write_inline(child)?;
                }
                self.end_tag("del")?;
                Ok(())
            }
            Inline::Group(content) => {
                for child in content {
                    self.write_inline(child)?;
                }
                Ok(())
            }
            Inline::InlineCode(code) => {
                self.start_tag("code")?;
                self.finish_tag()?;
                self.text(code)?;
                self.end_tag("code")?;
                Ok(())
            }
            Inline::Link {
                url,
                title,
                content,
            } => {
                self.start_tag("a")?;
                self.attribute("href", url)?;
                if let Some(title) = title {
                    if !title.is_empty() {
                        self.attribute("title", title)?;
                    }
                }
                self.finish_tag()?;
                for child in content {
                    self.write_inline(child)?;
                }
                self.end_tag("a")?;
                Ok(())
            }
            Inline::Image { url, title, alt } => {
                self.start_tag("img")?;
                self.attribute("src", url)?;
                let alt_text = render_inlines_to_plain_text_string(alt);
                self.attribute("alt", &alt_text)?;
                if let Some(title) = title {
                    if !title.is_empty() {
                        self.attribute("title", title)?;
                    }
                }
                self.finish_self_closing_tag()?;
                Ok(())
            }
            Inline::ReferenceLink { label, content } => {
                if self.options.strict {
                    return Err(format!(
                        "Unresolved reference link '[{}{}]' found in strict mode.",
                        render_inlines_to_plain_text_string(content),
                        label
                    )
                    .into());
                }

                self.text("[")?;
                let content_text = render_inlines_to_plain_text_string(content);
                if content.is_empty() || content_text == *label {
                    self.text(label)?;
                } else {
                    for child in content {
                        self.write_inline(child)?;
                    }
                }
                self.text("]")?;
                if !(content.is_empty() && label.is_empty()
                    || content_text == *label
                        && content.len() == 1
                        && matches!(content[0], Inline::Text(_)))
                {
                    let is_explicit_full_or_collapsed_form = !content.is_empty();
                    if is_explicit_full_or_collapsed_form {
                        self.text("[")?;
                        self.text(label)?;
                        self.text("]")?;
                    }
                }
                Ok(())
            }
            Inline::Autolink { url, is_email } => {
                self.start_tag("a")?;
                let href = if *is_email && !url.starts_with("mailto:") {
                    format!("mailto:{url}")
                } else {
                    url.to_string()
                };
                self.attribute("href", &href)?;
                self.finish_tag()?;
                self.text(url)?;
                self.end_tag("a")?;
                Ok(())
            }
            Inline::SoftBreak => self.write_trusted_html("\n"),
            Inline::HardBreak => {
                self.self_closing_tag("br")?;
                self.write_trusted_html("\n")
            }
            Inline::HtmlElement(element) => self.write_html_element(element),
            Inline::Highlight(content) => {
                let element = ir::HtmlElement {
                    tag: EcoString::inline("mark"),
                    attributes: vec![],
                    children: content.iter().cloned().map(IrNode::Inline).collect(),
                    self_closing: false,
                };
                self.write_html_element(&element)
            }
            Inline::Verbatim(text) => self.write_trusted_html(text),
            Inline::Comment(text) => self.write_trusted_html(&format!("<!-- {text} -->")),
            Inline::EmbeddedBlock(block) => self.write_block(block),
            Inline::UnsupportedCustom => Ok(()),
        }
    }
}

fn render_inlines_to_plain_text_string(inlines: &[Inline]) -> EcoString {
    let mut out = EcoString::new();
    render_inlines_to_plain_text(inlines, &mut out);
    out
}

fn render_inlines_to_plain_text(inlines: &[Inline], out: &mut EcoString) {
    fn push_space(out: &mut EcoString) {
        if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
    }

    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::InlineCode(code) => out.push_str(code),
            Inline::Emphasis(content)
            | Inline::Strong(content)
            | Inline::Strikethrough(content)
            | Inline::Group(content)
            | Inline::Highlight(content) => render_inlines_to_plain_text(content, out),
            Inline::Link { content, .. } | Inline::ReferenceLink { content, .. } => {
                render_inlines_to_plain_text(content, out)
            }
            Inline::Image { alt, .. } => render_inlines_to_plain_text(alt, out),
            Inline::Autolink { url, .. } => out.push_str(url),
            Inline::SoftBreak | Inline::HardBreak => push_space(out),
            Inline::HtmlElement(element) => {
                for child in &element.children {
                    match child {
                        IrNode::Inline(inline) => {
                            render_inlines_to_plain_text(std::slice::from_ref(inline), out)
                        }
                        IrNode::Block(_) => {}
                    }
                }
            }
            Inline::Verbatim(text) => out.push_str(text),
            Inline::Comment(_) => {}
            Inline::EmbeddedBlock(_) => push_space(out),
            Inline::UnsupportedCustom => {}
        }
    }
}
