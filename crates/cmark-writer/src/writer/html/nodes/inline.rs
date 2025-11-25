use super::super::utils::{render_nodes_to_plain_text, render_nodes_to_plain_text_string};
use super::super::{HtmlWriteError, HtmlWriteResult, HtmlWriter};
use crate::ast::Node;
use ecow::EcoString;

impl HtmlWriter {
    pub(crate) fn write_text(&mut self, text: &str) -> HtmlWriteResult<()> {
        self.text(text)
    }

    pub(crate) fn write_emphasis(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag("em")?;
        self.finish_tag()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag("em")?;
        Ok(())
    }

    pub(crate) fn write_strong(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        self.start_tag("strong")?;
        self.finish_tag()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag("strong")?;
        Ok(())
    }

    pub(crate) fn write_inline_code(&mut self, code: &str) -> HtmlWriteResult<()> {
        self.start_tag("code")?;
        self.finish_tag()?;
        self.text(code)?;
        self.end_tag("code")?;
        Ok(())
    }

    pub(crate) fn write_soft_break(&mut self) -> HtmlWriteResult<()> {
        self.write_trusted_html("\n")
    }

    pub(crate) fn write_hard_break(&mut self) -> HtmlWriteResult<()> {
        self.self_closing_tag("br")?;
        self.write_trusted_html("\n")
    }

    pub(crate) fn write_link(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        content: &[Node],
    ) -> HtmlWriteResult<()> {
        self.start_tag("a")?;
        self.attribute("href", url)?;
        if let Some(title_str) = title {
            if !title_str.is_empty() {
                self.attribute("title", title_str)?;
            }
        }
        self.finish_tag()?;
        for child in content {
            self.write_node(child)?;
        }
        self.end_tag("a")?;
        Ok(())
    }

    pub(crate) fn write_image(
        &mut self,
        url: &str,
        title: &Option<EcoString>,
        alt: &[Node],
    ) -> HtmlWriteResult<()> {
        self.start_tag("img")?;
        self.attribute("src", url)?;
        let mut alt_text_buffer = EcoString::new();
        render_nodes_to_plain_text(alt, &mut alt_text_buffer);
        self.attribute("alt", &alt_text_buffer)?;
        if let Some(title_str) = title {
            if !title_str.is_empty() {
                self.attribute("title", title_str)?;
            }
        }
        self.finish_self_closing_tag()?;
        Ok(())
    }

    pub(crate) fn write_autolink(&mut self, url: &str, is_email: bool) -> HtmlWriteResult<()> {
        self.start_tag("a")?;
        let href = if is_email && !url.starts_with("mailto:") {
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

    #[cfg(feature = "gfm")]
    pub(crate) fn write_extended_autolink(&mut self, url: &str) -> HtmlWriteResult<()> {
        if !self.options.enable_gfm {
            self.emit_warning(
                "ExtendedAutolink node encountered but GFM (or GFM autolinks) is not enabled. Rendering as plain text.".
                    to_string(),
            );
            self.text(url)?;
            return Ok(());
        }
        self.start_tag("a")?;
        self.attribute("href", url)?;
        self.finish_tag()?;
        self.text(url)?;
        self.end_tag("a")?;
        Ok(())
    }

    pub(crate) fn write_reference_link(
        &mut self,
        label: &str,
        content: &[Node],
    ) -> HtmlWriteResult<()> {
        if self.options.strict {
            return Err(HtmlWriteError::UnsupportedNodeType(format!(
                "Unresolved reference link '[{}{}]' found in strict mode. Pre-resolve links for HTML output.",
                render_nodes_to_plain_text_string(content),
                label
            )));
        }

        self.emit_warning(format!(
            "Unresolved reference link for label '{label}'. Rendering as plain text."
        ));
        self.text("[")?;
        let content_text = render_nodes_to_plain_text_string(content);
        if content.is_empty() || content_text == label {
            self.text(label)?;
        } else {
            for node_in_content in content {
                self.write_node(node_in_content)?;
            }
        }
        self.text("]")?;
        if !(content.is_empty() && label.is_empty()
            || content_text == label && content.len() == 1 && matches!(content[0], Node::Text(_)))
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

    #[cfg(feature = "gfm")]
    pub(crate) fn write_strikethrough(&mut self, children: &[Node]) -> HtmlWriteResult<()> {
        if !self.options.enable_gfm {
            self.emit_warning(
                "Strikethrough node encountered but GFM (or GFM strikethrough) is not enabled. Rendering content as plain.".
                    to_string(),
            );
            for child in children {
                self.write_node(child)?;
            }
            return Ok(());
        }
        self.start_tag("del")?;
        self.finish_tag()?;
        for child in children {
            self.write_node(child)?;
        }
        self.end_tag("del")?;
        Ok(())
    }
}
