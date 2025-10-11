use crate::ast::Node;
use crate::error::{WriteError, WriteResult};
use crate::writer::html::{HtmlWriter, HtmlWriterOptions};
use ecow::EcoString;

use super::utils::{escape_str, CommonMarkEscapes};
use super::CommonMarkWriter;

impl CommonMarkWriter {
    /// Write text content, escaping special characters when configured.
    pub(crate) fn write_text_content(&mut self, content: &str) -> WriteResult<()> {
        if self.options.escape_special_chars {
            let escaped = escape_str::<CommonMarkEscapes>(content);
            self.write_str(&escaped)?;
        } else {
            self.write_str(content)?;
        }
        Ok(())
    }

    /// Write inline code surrounded by backticks.
    pub(crate) fn write_code_content(&mut self, content: &str) -> WriteResult<()> {
        self.write_char('`')?;
        self.write_str(content)?;
        self.write_char('`')?;
        Ok(())
    }

    /// Write inline content surrounded by a delimiter.
    pub(crate) fn write_delimited(&mut self, content: &[Node], delimiter: &str) -> WriteResult<()> {
        self.write_str(delimiter)?;
        for node in content {
            self.write(node)?;
        }
        self.write_str(delimiter)?;
        Ok(())
    }

    /// Write an emphasis (italic) span.
    pub(crate) fn write_emphasis(&mut self, content: &[Node]) -> WriteResult<()> {
        let delimiter = self.options.emphasis_char.to_string();
        self.write_delimited(content, &delimiter)
    }

    /// Write a strong (bold) span.
    pub(crate) fn write_strong(&mut self, content: &[Node]) -> WriteResult<()> {
        let ch = self.options.strong_char;
        let delimiter = format!("{ch}{ch}");
        self.write_delimited(content, &delimiter)
    }

    /// Write a strikethrough span (GFM).
    #[cfg(feature = "gfm")]
    pub(crate) fn write_strikethrough(&mut self, content: &[Node]) -> WriteResult<()> {
        if !self.options.enable_gfm || !self.options.gfm_strikethrough {
            for node in content {
                self.write(node)?;
            }
            return Ok(());
        }
        self.write_delimited(content, "~~")
    }

    /// Write a link, validating that the label has no newlines.
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
            self.write_char('\"')?;
        }

        self.write_char(')')?;
        Ok(())
    }

    /// Write an image with alt text validation.
    pub(crate) fn write_image(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> WriteResult<()> {
        for node in alt {
            self.check_no_newline(node, "Image alt text")?;
        }

        self.write_str("![")?;
        for node in alt {
            self.write(node)?;
        }
        self.write_str("](")?;
        self.write_str(url)?;

        if let Some(title_text) = title {
            self.write_str(" \"")?;
            self.write_str(title_text)?;
            self.write_char('\"')?;
        }

        self.write_char(')')?;
        Ok(())
    }

    /// Write a soft line break.
    pub(crate) fn write_soft_break(&mut self) -> WriteResult<()> {
        self.write_char('\n')?;
        Ok(())
    }

    /// Write a hard line break honouring configured style.
    pub(crate) fn write_hard_break(&mut self) -> WriteResult<()> {
        if self.options.hard_break_spaces {
            self.write_str("  \n")?;
        } else {
            self.write_str("\\\n")?;
        }
        Ok(())
    }

    /// Write an autolink (URI/email) wrapped in angle brackets.
    pub(crate) fn write_autolink(&mut self, url: &str, is_email: bool) -> WriteResult<()> {
        let _ = is_email;
        if url.contains('\n') {
            if self.is_strict_mode() {
                return Err(WriteError::NewlineInInlineElement(
                    "Autolink URL".to_string().into(),
                ));
            } else {
                self.emit_warning(format!(
                    "Newline character found in autolink URL '{url}'. Writing it as is, which might result in an invalid link."
                ));
            }
        }

        self.write_char('<')?;
        self.write_str(url)?;
        self.write_char('>')?;
        Ok(())
    }

    /// Write an extended autolink (GFM) or fall back to plain text.
    #[cfg(feature = "gfm")]
    pub(crate) fn write_extended_autolink(&mut self, url: &str) -> WriteResult<()> {
        if !self.options.gfm_autolinks {
            self.write_text_content(url)?;
            return Ok(());
        }

        if url.contains('\n') {
            if self.is_strict_mode() {
                return Err(WriteError::NewlineInInlineElement(
                    "Extended Autolink URL".to_string().into(),
                ));
            } else {
                self.emit_warning(format!(
                    "Newline character found in extended autolink URL '{url}'. Writing it as is, which might result in an invalid link."
                ));
            }
        }

        self.write_str(url)?;
        Ok(())
    }

    /// Write a link reference definition.
    pub(crate) fn write_link_reference_definition(
        &mut self,
        label: &str,
        destination: &str,
        title: &Option<EcoString>,
    ) -> WriteResult<()> {
        self.write_char('[')?;
        self.write_str(label)?;
        self.write_str("]: ")?;
        self.write_str(destination)?;

        if let Some(title_text) = title {
            self.write_str(" \"")?;
            self.write_str(title_text)?;
            self.write_char('\"')?;
        }

        Ok(())
    }

    /// Write a reference link, using shortcut syntax when possible.
    pub(crate) fn write_reference_link(
        &mut self,
        label: &str,
        content: &[Node],
    ) -> WriteResult<()> {
        for node in content {
            self.check_no_newline(node, "Reference Link Text")?;
        }

        if content.is_empty() {
            self.write_char('[')?;
            self.write_str(label)?;
            self.write_char(']')?;
            return Ok(());
        }

        let is_shortcut =
            content.len() == 1 && matches!(&content[0], Node::Text(text) if text == label);

        if is_shortcut {
            self.write_char('[')?;
            self.write_str(label)?;
            self.write_char(']')?;
        } else {
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

    /// Write an AST HtmlElement node using the HTML writer.
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

        let html_options = if let Some(ref custom) = self.options.html_writer_options {
            custom.clone()
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
        let html_output = html_writer.into_string();
        self.write_str(&html_output)
    }
}
