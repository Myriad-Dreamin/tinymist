use super::{utils, HtmlWriteError, HtmlWriteResult, HtmlWriterOptions};
use crate::ast::{HtmlElement, ListItem, Node};
#[cfg(feature = "gfm")]
use crate::ast::{TableAlignment, TaskListStatus};
use ecow::EcoString;
use html_escape;
use log;

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
/// let output = writer.into_string();
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
/// let output = writer.into_string();
/// assert_eq!(output, "<div class=\"container\"><h1>Welcome</h1></div>");
/// ```
#[derive(Debug)]
pub struct HtmlWriter {
    /// Writer options
    pub options: HtmlWriterOptions,
    /// Buffer for storing the output text
    buffer: EcoString,
    /// Whether a tag is currently opened
    tag_opened: bool,
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
        }
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
    pub fn into_string(mut self) -> EcoString {
        self.ensure_tag_closed().unwrap();
        self.buffer
    }

    // --- Low-level HTML writing primitives ---

    fn ensure_tag_closed(&mut self) -> HtmlWriteResult<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    fn start_tag_internal(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.tag_opened = true;
        Ok(())
    }

    /// Starts an HTML tag with the given name.
    ///
    /// This is a public wrapper around start_tag_internal.
    pub fn start_tag(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.start_tag_internal(tag_name)
    }

    fn attribute_internal(&mut self, key: &str, value: &str) -> HtmlWriteResult<()> {
        if !self.tag_opened {
            return Err(HtmlWriteError::InvalidHtmlTag(
                "Cannot write attribute: no tag is currently open.".to_string(),
            ));
        }
        self.buffer.push(' ');
        self.buffer.push_str(key);
        self.buffer.push_str("=\"");
        self.buffer
            .push_str(html_escape::encode_text(value).as_ref());
        self.buffer.push('"');
        Ok(())
    }

    /// Adds an attribute to the currently open tag.
    ///
    /// This is a public wrapper around attribute_internal.
    pub fn attribute(&mut self, key: &str, value: &str) -> HtmlWriteResult<()> {
        self.attribute_internal(key, value)
    }

    fn finish_tag_internal(&mut self) -> HtmlWriteResult<()> {
        if self.tag_opened {
            self.buffer.push('>');
            self.tag_opened = false;
        }
        Ok(())
    }

    /// Finishes the current open tag.
    ///
    /// This is a public wrapper around finish_tag_internal.
    pub fn finish_tag(&mut self) -> HtmlWriteResult<()> {
        self.finish_tag_internal()
    }

    fn end_tag_internal(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str("</");
        self.buffer.push_str(tag_name);
        self.buffer.push('>');
        Ok(())
    }

    /// Closes an HTML tag with the given name.
    ///
    /// This is a public wrapper around end_tag_internal.
    pub fn end_tag(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.end_tag_internal(tag_name)
    }

    fn text_internal(&mut self, text: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer
            .push_str(html_escape::encode_text(text).as_ref());
        Ok(())
    }

    /// Writes text content, escaping HTML special characters.
    ///
    /// This is a public wrapper around text_internal.
    pub fn text(&mut self, text: &str) -> HtmlWriteResult<()> {
        self.text_internal(text)
    }

    /// Writes a string to the output, escaping HTML special characters.
    ///
    /// This is an alias for `text` method, provided for compatibility with
    /// the CustomNodeWriter trait interface.
    pub fn write_str(&mut self, s: &str) -> HtmlWriteResult<()> {
        self.text(s)
    }

    fn self_closing_tag_internal(&mut self, tag_name: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push('<');
        self.buffer.push_str(tag_name);
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    fn finish_self_closing_tag_internal(&mut self) -> HtmlWriteResult<()> {
        if !self.tag_opened {
            return Err(HtmlWriteError::InvalidHtmlTag(
                "Cannot finish self-closing tag: no tag is currently open.".to_string(),
            ));
        }
        self.buffer.push_str(" />");
        self.tag_opened = false;
        Ok(())
    }

    /// Finishes the current open tag as a self-closing tag.
    ///
    /// This is a public wrapper around finish_self_closing_tag_internal.
    pub fn finish_self_closing_tag(&mut self) -> HtmlWriteResult<()> {
        self.finish_self_closing_tag_internal()
    }

    fn raw_html_internal(&mut self, html: &str) -> HtmlWriteResult<()> {
        self.ensure_tag_closed()?;
        self.buffer.push_str(html);
        Ok(())
    }

    /// Writes raw HTML content directly to the output.
    ///
    /// This method allows adding arbitrary HTML content without escaping.
    /// It should be used with caution as it can introduce security issues
    /// if used with untrusted input.
    pub fn raw_html(&mut self, html: &str) -> HtmlWriteResult<()> {
        self.raw_html_internal(html)
    }

    // --- Main Node Dispatcher ---

    /// Writes an AST `Node` to HTML using the configured options.
    pub fn write_node(&mut self, node: &Node) -> HtmlWriteResult<()> {
        match node {
            Node::Document(children) => self.write_document_node(children),
            Node::Paragraph(children) => self.write_paragraph_node(children),
            Node::Text(text) => self.write_text_node(text),
            Node::Heading { level, content, .. } => self.write_heading_node(*level, content),
            Node::Emphasis(children) => self.write_emphasis_node(children),
            Node::Strong(children) => self.write_strong_node(children),
            Node::ThematicBreak => self.write_thematic_break_node(),
            Node::InlineCode(code) => self.write_inline_code_node(code),
            Node::CodeBlock {
                language, content, ..
            } => self.write_code_block_node(language, content),
            Node::HtmlBlock(block_content) => self.write_html_block_node(block_content),
            Node::HtmlElement(element) => self.write_html_element_node(element),
            Node::SoftBreak => self.write_soft_break_node(),
            Node::HardBreak => self.write_hard_break_node(),
            Node::Link {
                url,
                title,
                content,
            } => self.write_link_node(url, title, content),
            Node::Image { url, title, alt } => self.write_image_node(url, title, alt),
            Node::BlockQuote(children) => self.write_blockquote_node(children),
            Node::OrderedList { start, items } => self.write_ordered_list_node(*start, items),
            Node::UnorderedList(items) => self.write_unordered_list_node(items),
            #[cfg(feature = "gfm")]
            Node::Strikethrough(children) => self.write_strikethrough_node(children),
            Node::Table {
                headers,
                #[cfg(feature = "gfm")]
                alignments,
                rows,
            } => self.write_table_node(
                headers,
                #[cfg(feature = "gfm")]
                alignments,
                rows,
            ),
            Node::Autolink { url, is_email } => self.write_autolink_node(url, *is_email),
            #[cfg(feature = "gfm")]
            Node::ExtendedAutolink(url) => self.write_extended_autolink_node(url),
            Node::LinkReferenceDefinition { .. } => Ok(()), // Definitions are not rendered in final HTML
            Node::ReferenceLink { label, content } => {
                self.write_reference_link_node(label, content)
            }
            Node::Custom(custom_node) => {
                // Call the CustomNode's html_write method, which handles the HTML rendering
                custom_node.html_write(self)
            }
            // Fallback for node types not handled, especially if GFM is off and GFM nodes appear
            #[cfg(not(feature = "gfm"))]
            Node::ExtendedAutolink(url) => {
                // Handle GFM specific nodes explicitly if feature is off
                log::warn!("ExtendedAutolink encountered but GFM feature is not enabled. Rendering as text: {url}");
                self.text_internal(url)
            }
            // All node types are handled above, but keeping this for future extensibility
            #[allow(unreachable_patterns)]
            _ => Err(HtmlWriteError::UnsupportedNodeType(format!("{node:?}"))),
        }
    }

    // --- Node-Specific Writing Methods (Internal) ---

    fn write_document_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        for child in children {
            self.write_node(child)?;
            // Optionally add newlines between major block elements in HTML source
            if child.is_block() && !self.buffer.ends_with('\n') {
                // self.raw_html_internal("\n")?;
            }
        }
        Ok(())
    }

    fn write_paragraph_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag_internal("p")?;
        self.finish_tag_internal()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag_internal("p")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_text_node(&mut self, text: &str) -> HtmlWriteResult<()> {
        self.text_internal(text)
    }

    fn write_heading_node(&mut self, level: u8, content: &[Node]) -> HtmlWriteResult<()> {
        let tag_name = format!("h{}", level.clamp(1, 6));
        self.start_tag_internal(&tag_name)?;
        self.finish_tag_internal()?;
        for child in content {
            self.write_node(child)?;
        }
        self.end_tag_internal(&tag_name)?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_emphasis_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag_internal("em")?;
        self.finish_tag_internal()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag_internal("em")?;
        Ok(())
    }

    fn write_strong_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag_internal("strong")?;
        self.finish_tag_internal()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag_internal("strong")?;
        Ok(())
    }

    fn write_thematic_break_node(&mut self) -> HtmlWriteResult<()> {
        self.self_closing_tag_internal("hr")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_inline_code_node(&mut self, code: &str) -> HtmlWriteResult<()> {
        self.start_tag_internal("code")?;
        self.finish_tag_internal()?;
        self.text_internal(code)?;
        self.end_tag_internal("code")?;
        Ok(())
    }

    fn write_code_block_node(
        &mut self,
        language: &Option<EcoString>,
        content: &str,
    ) -> HtmlWriteResult<()> {
        self.start_tag_internal("pre")?;
        self.finish_tag_internal()?; // Finish <pre> before potentially adding attributes to <code> or <span>
        self.start_tag_internal("code")?;
        if let Some(prefix) = &self.options.code_block_language_class_prefix {
            if let Some(lang) = language {
                if !lang.is_empty() {
                    self.attribute_internal("class", &format!("{}{}", prefix, lang.trim()))?;
                }
            }
        }
        self.finish_tag_internal()?;
        self.text_internal(content)?;
        self.end_tag_internal("code")?;
        self.end_tag_internal("pre")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_html_block_node(&mut self, block_content: &str) -> HtmlWriteResult<()> {
        self.raw_html_internal(block_content)?;
        if !block_content.ends_with('\n') {
            self.raw_html_internal("\n")?;
        }
        Ok(())
    }

    fn write_html_element_node(&mut self, element: &HtmlElement) -> HtmlWriteResult<()> {
        #[cfg(feature = "gfm")]
        if self.options.enable_gfm
            && self
                .options
                .gfm_disallowed_html_tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(&element.tag))
        {
            log::debug!("GFM: Textualizing disallowed HTML tag: <{}>", element.tag);
            self.textualize_full_element_node(element)?;
            return Ok(());
        }

        if !utils::is_safe_tag_name(&element.tag) {
            if self.options.strict {
                return Err(HtmlWriteError::InvalidHtmlTag(element.tag.to_string()));
            } else {
                log::warn!(
                    "Invalid HTML tag name '{}' encountered. Textualizing in non-strict mode.",
                    element.tag
                );
                self.textualize_full_element_node(element)?;
                return Ok(());
            }
        }

        self.start_tag_internal(&element.tag)?;
        for attr in &element.attributes {
            if !utils::is_safe_attribute_name(&attr.name) {
                if self.options.strict {
                    return Err(HtmlWriteError::InvalidHtmlAttribute(attr.name.to_string()));
                } else {
                    log::warn!("Invalid HTML attribute name '{}' in tag '{}'. Textualizing attribute in non-strict mode.", attr.name, element.tag);
                    // Simple textualization of the attribute itself
                    self.buffer.push(' ');
                    self.buffer.push_str(&attr.name);
                    self.buffer.push_str("=\"");
                    self.buffer
                        .push_str(html_escape::encode_text(&attr.value).as_ref()); // Attribute value should be escaped
                    self.buffer.push('"');
                    continue;
                }
            }
            self.attribute_internal(&attr.name, &attr.value)?;
        }

        if element.self_closing {
            self.finish_self_closing_tag_internal()?;
        } else {
            self.finish_tag_internal()?;
            for child in &element.children {
                self.write_node(child)?;
            }
            self.end_tag_internal(&element.tag)?;
        }
        // Determine if a newline is appropriate based on whether the original HTML tag is block or inline.
        // This information isn't readily available, so we might add a newline if it's a common block tag.
        // For now, omitting conditional newline for brevity.
        Ok(())
    }

    fn textualize_full_element_node(&mut self, element: &HtmlElement) -> HtmlWriteResult<()> {
        self.text_internal("<")?;
        self.text_internal(&element.tag)?;
        for attr in &element.attributes {
            self.text_internal(" ")?;
            self.text_internal(&attr.name)?;
            self.text_internal("=")?;
            self.text_internal("\"")?;
            self.text_internal(&attr.value)?; // Value is part of the text, so it's escaped by text_internal
            self.text_internal("\"")?;
        }
        if element.self_closing {
            self.text_internal(" />")?;
        } else {
            self.text_internal(">")?;
            for child in &element.children {
                self.write_node(child)?; // Children are rendered normally
            }
            self.text_internal("</")?;
            self.text_internal(&element.tag)?;
            self.text_internal(">")?;
        }
        Ok(())
    }

    fn write_soft_break_node(&mut self) -> HtmlWriteResult<()> {
        // Soft line breaks in CommonMark are rendered as a newline in HTML source,
        // or a space if the line break was for wrapping.
        // Most browsers will treat a newline in HTML as a single space.
        self.raw_html_internal("\n")
    }

    fn write_hard_break_node(&mut self) -> HtmlWriteResult<()> {
        self.self_closing_tag_internal("br")?;
        self.raw_html_internal("\n")
    }

    fn write_link_node(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        content: &[Node],
    ) -> HtmlWriteResult<()> {
        self.start_tag_internal("a")?;
        self.attribute_internal("href", url)?;
        if let Some(title_str) = title {
            if !title_str.is_empty() {
                self.attribute_internal("title", title_str)?;
            }
        }
        self.finish_tag_internal()?;
        for child in content {
            self.write_node(child)?;
        }
        self.end_tag_internal("a")?;
        Ok(())
    }

    fn write_image_node(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> HtmlWriteResult<()> {
        self.start_tag_internal("img")?;
        self.attribute_internal("src", url)?;
        let mut alt_text_buffer = EcoString::new();
        render_nodes_to_plain_text(alt, &mut alt_text_buffer, &self.options);
        self.attribute_internal("alt", &alt_text_buffer)?;
        if let Some(title_str) = title {
            if !title_str.is_empty() {
                self.attribute_internal("title", title_str)?;
            }
        }
        self.finish_self_closing_tag_internal()?;
        Ok(())
    }

    fn write_blockquote_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag_internal("blockquote")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag_internal("blockquote")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_list_item_node_content(&mut self, item_content: &[Node]) -> HtmlWriteResult<()> {
        // This is a simplified handling. CommonMark's "tight" vs "loose" list rules
        // determine if paragraph tags are used inside <li> for paragraphs.
        // If the first/only child of <li> content is a paragraph, and it's a tight list,
        // the <p> tags are often omitted.
        // This implementation will render <p> if Node::Paragraph is present.
        let mut add_newline_before_next_child = false;
        for child_node in item_content.iter() {
            if add_newline_before_next_child {
                self.raw_html_internal("\n")?;
                add_newline_before_next_child = false;
            }
            self.write_node(child_node)?;
            if child_node.is_block() {
                add_newline_before_next_child = true;
            }
        }
        Ok(())
    }

    fn write_list_item_node(&mut self, item: &ListItem) -> HtmlWriteResult<()> {
        self.start_tag_internal("li")?;

        #[cfg(feature = "gfm")]
        if self.options.enable_gfm {
            if let ListItem::Task { status, .. } = item {
                let class_name = if *status == TaskListStatus::Checked {
                    "task-list-item task-list-item-checked"
                } else {
                    "task-list-item" // GFM spec doesn't mandate a specific class for unchecked, just for checked.
                                     // Some renderers use task-list-item-unchecked.
                };
                self.attribute_internal("class", class_name)?;
            }
        }
        self.finish_tag_internal()?; // Finish <li> tag

        let content = match item {
            ListItem::Unordered { content } => content,
            ListItem::Ordered { content, .. } => content,
            #[cfg(feature = "gfm")]
            ListItem::Task { content, .. } => content,
        };

        #[cfg(feature = "gfm")]
        if self.options.enable_gfm {
            if let ListItem::Task { status, .. } = item {
                self.start_tag_internal("input")?;
                self.attribute_internal("type", "checkbox")?;
                self.attribute_internal("disabled", "")?; // GFM task list items are disabled
                if *status == TaskListStatus::Checked {
                    self.attribute_internal("checked", "")?;
                }
                self.finish_self_closing_tag_internal()?;
                self.raw_html_internal(" ")?; // Space after checkbox
            }
        }
        self.write_list_item_node_content(content)?;
        self.end_tag_internal("li")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_ordered_list_node(&mut self, start: u32, items: &[ListItem]) -> HtmlWriteResult<()> {
        self.start_tag_internal("ol")?;
        if start != 1 {
            self.attribute_internal("start", &start.to_string())?;
        }
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;
        for item in items {
            self.write_list_item_node(item)?;
        }
        self.end_tag_internal("ol")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_unordered_list_node(&mut self, items: &[ListItem]) -> HtmlWriteResult<()> {
        self.start_tag_internal("ul")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;
        for item in items {
            self.write_list_item_node(item)?;
        }
        self.end_tag_internal("ul")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    #[cfg(feature = "gfm")]
    fn write_strikethrough_node(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        if !self.options.enable_gfm {
            // If GFM is disabled (e.g. via a more granular gfm_strikethrough option if added),
            // render content as is. This case should ideally be guarded by options check.
            log::warn!("Strikethrough node encountered but GFM (or GFM strikethrough) is not enabled. Rendering content as plain.");
            for child in children {
                self.write_node(child)?;
            }
            return Ok(());
        }
        self.start_tag_internal("del")?;
        self.finish_tag_internal()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag_internal("del")?;
        Ok(())
    }

    fn write_table_node(
        &mut self,
        headers: &[Node],
        #[cfg(feature = "gfm")] alignments: &[TableAlignment],
        rows: &[Vec<Node>],
    ) -> HtmlWriteResult<()> {
        self.start_tag_internal("table")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;

        // Table Head
        self.start_tag_internal("thead")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;
        self.start_tag_internal("tr")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;

        // Write header cells
        #[cfg(feature = "gfm")]
        for (col_index, header_cell) in headers.iter().enumerate() {
            self.start_tag_internal("th")?;

            // Apply alignment styles if GFM is enabled
            if self.options.enable_gfm && col_index < alignments.len() {
                match alignments[col_index] {
                    TableAlignment::Left => {
                        self.attribute_internal("style", "text-align: left;")?;
                    }
                    TableAlignment::Center => {
                        self.attribute_internal("style", "text-align: center;")?;
                    }
                    TableAlignment::Right => {
                        self.attribute_internal("style", "text-align: right;")?;
                    }
                    TableAlignment::None => {}
                }
            }

            self.finish_tag_internal()?;
            self.write_node(header_cell)?;
            self.end_tag_internal("th")?;
            self.raw_html_internal("\n")?;
        }

        #[cfg(not(feature = "gfm"))]
        for header_cell in headers.iter() {
            self.start_tag_internal("th")?;
            self.finish_tag_internal()?;
            self.write_node(header_cell)?;
            self.end_tag_internal("th")?;
            self.raw_html_internal("\n")?;
        }
        self.end_tag_internal("tr")?;
        self.raw_html_internal("\n")?;
        self.end_tag_internal("thead")?;
        self.raw_html_internal("\n")?;

        // Table Body
        self.start_tag_internal("tbody")?;
        self.finish_tag_internal()?;
        self.raw_html_internal("\n")?;

        // Process each row
        for row_cells in rows {
            self.start_tag_internal("tr")?;
            self.finish_tag_internal()?;
            self.raw_html_internal("\n")?;

            // Process cells with alignment support when GFM is enabled
            #[cfg(feature = "gfm")]
            for (col_index, cell) in row_cells.iter().enumerate() {
                self.start_tag_internal("td")?;

                // Apply alignment styles if GFM is enabled
                if self.options.enable_gfm && col_index < alignments.len() {
                    match alignments[col_index] {
                        TableAlignment::Left => {
                            self.attribute_internal("style", "text-align: left;")?;
                        }
                        TableAlignment::Center => {
                            self.attribute_internal("style", "text-align: center;")?;
                        }
                        TableAlignment::Right => {
                            self.attribute_internal("style", "text-align: right;")?;
                        }
                        TableAlignment::None => {}
                    }
                }

                self.finish_tag_internal()?;
                self.write_node(cell)?;
                self.end_tag_internal("td")?;
                self.raw_html_internal("\n")?;
            }

            // Process cells normally when GFM is not enabled
            #[cfg(not(feature = "gfm"))]
            for cell in row_cells.iter() {
                self.start_tag_internal("td")?;
                self.finish_tag_internal()?;
                self.write_node(cell)?;
                self.end_tag_internal("td")?;
                self.raw_html_internal("\n")?;
            }

            self.end_tag_internal("tr")?;
            self.raw_html_internal("\n")?;
        }

        self.end_tag_internal("tbody")?;
        self.raw_html_internal("\n")?;
        self.end_tag_internal("table")?;
        self.raw_html_internal("\n")?;
        Ok(())
    }

    fn write_autolink_node(&mut self, url: &str, is_email: bool) -> HtmlWriteResult<()> {
        self.start_tag_internal("a")?;
        let href = if is_email && !url.starts_with("mailto:") {
            format!("mailto:{url}")
        } else {
            url.to_string()
        };
        self.attribute_internal("href", &href)?;
        self.finish_tag_internal()?;
        self.text_internal(url)?;
        self.end_tag_internal("a")?;
        Ok(())
    }

    #[cfg(feature = "gfm")]
    fn write_extended_autolink_node(&mut self, url: &str) -> HtmlWriteResult<()> {
        if !self.options.enable_gfm {
            // Or a more specific gfm_autolinks option
            log::warn!("ExtendedAutolink node encountered but GFM (or GFM autolinks) is not enabled. Rendering as plain text.");
            self.text_internal(url)?;
            return Ok(());
        }
        self.start_tag_internal("a")?;
        self.attribute_internal("href", url)?; // Assumes URL is already a valid href
        self.finish_tag_internal()?;
        self.text_internal(url)?;
        self.end_tag_internal("a")?;
        Ok(())
    }

    fn write_reference_link_node(&mut self, label: &str, content: &[Node]) -> HtmlWriteResult<()> {
        // HTML rendering expects links to be resolved. If a ReferenceLink node is still present,
        // it means resolution failed or wasn't performed.
        // CommonMark dictates rendering the source text.
        if self.options.strict {
            return Err(HtmlWriteError::UnsupportedNodeType(format!(
                "Unresolved reference link '[{}{}]' found in strict mode. Pre-resolve links for HTML output.",
                render_nodes_to_plain_text_string(content, &self.options), // Get text of content
                label
            )));
        }

        log::warn!("Unresolved reference link for label '{label}'. Rendering as plain text.");
        // Render as plain text: [content][label] or [label]
        self.text_internal("[")?;
        let content_text = render_nodes_to_plain_text_string(content, &self.options);
        if content.is_empty() || content_text == label {
            // Handle [label] and [label][]
            self.text_internal(label)?;
        } else {
            // Handle [text][label]
            // In this case, `content` is rendered as its textual representation inside the first brackets
            // This might mean rendering resolved inline nodes within `content` as text.
            for node_in_content in content {
                self.write_node(node_in_content)?; // This will render HTML if content has e.g. <em>
            }
        }
        self.text_internal("]")?; // Closing first bracket set
                                  // If it's not a collapsed reference `[label]` and not `[text][]` (where label is implicitly text)
                                  // then we need the `[label]` part.
                                  // Shortcut `[label]` is already handled by `content_text == label`.
                                  // Full `[text][label]` requires the second bracket.
                                  // Collapsed `[text][]` implies label is derived from text, also handled.
                                  // This logic can be tricky; CommonMark source rendering is the safest fallback.
        if !(content_text == label && content.len() == 1 && matches!(content[0], Node::Text(_))) {
            // Avoid double [label][label] if content was just Node::Text(label)
            if !(content.is_empty() && label.is_empty()) {
                // Avoids `[][]` if both are empty (unlikely)
                // This part ensures `[label]` for `[text][label]` or `[label][]` forms
                // if content is not simply the label text itself.
                let is_explicit_full_or_collapsed_form = !content.is_empty(); // e.g. [foo][bar] or [baz][]
                if is_explicit_full_or_collapsed_form {
                    self.text_internal("[")?;
                    self.text_internal(label)?; // The actual reference label
                    self.text_internal("]")?;
                }
            }
        }
        Ok(())
    }
}

impl Default for HtmlWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to render a slice of AST nodes to a plain text string.
/// Used for 'alt' text in images or for textual representation of link content.
fn render_nodes_to_plain_text(
    nodes: &[Node],
    buffer: &mut EcoString,
    _options: &HtmlWriterOptions,
) {
    for node in nodes {
        match node {
            Node::Text(text) => buffer.push_str(text),
            Node::Emphasis(children) | Node::Strong(children) => {
                render_nodes_to_plain_text(children, buffer, _options);
            }
            #[cfg(feature = "gfm")]
            Node::Strikethrough(children) => {
                render_nodes_to_plain_text(children, buffer, _options);
            }
            Node::Link { content, .. } => render_nodes_to_plain_text(content, buffer, _options),
            Node::Image { alt, .. } => render_nodes_to_plain_text(alt, buffer, _options), // Recursively get alt text
            Node::InlineCode(code) => buffer.push_str(code),
            Node::SoftBreak | Node::HardBreak => buffer.push(' '), // Represent breaks as spaces in alt text
            Node::HtmlElement(element) => {
                // Strip HTML tags, but render their text content
                render_nodes_to_plain_text(&element.children, buffer, _options);
            }
            Node::Autolink { url, .. } | Node::ExtendedAutolink(url) => buffer.push_str(url),
            // Block elements are generally not expected in contexts like 'alt' text,
            // but if they are, extract their text content.
            Node::Paragraph(children)
            | Node::BlockQuote(children)
            | Node::Heading {
                content: children, ..
            } => {
                render_nodes_to_plain_text(children, buffer, _options);
                buffer.push(' '); // Add a space after block content for readability
            }
            _ => {} // Ignore other node types (e.g., ThematicBreak, Table, List) for plain text rendering.
        }
    }
}

/// Convenience wrapper for `render_nodes_to_plain_text` that returns a EcoString.
fn render_nodes_to_plain_text_string(nodes: &[Node], options: &HtmlWriterOptions) -> EcoString {
    let mut s = EcoString::new();
    render_nodes_to_plain_text(nodes, &mut s, options);
    s
}

// Example of how CustomNode's html_write might be expected by the HtmlWriter:
// (This would be part of the CustomNode trait definition and its implementations)
// pub trait CustomNode {
//     // ... other methods ...
//     fn html_write(&self, options: &HtmlRenderOptions) -> Result<EcoString, EcoString>; // EcoString for error msg
// }
