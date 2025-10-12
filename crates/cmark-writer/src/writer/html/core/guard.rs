use ecow::EcoString;

use super::{HtmlWriteError, HtmlWriteResult, HtmlWriter};
use crate::ast::HtmlAttribute;
use crate::writer::html::utils;

pub(crate) enum GuardedHtmlElement<'a> {
    Render(GuardedTagWriter<'a>),
    Textualize,
}

pub(crate) struct GuardedTagWriter<'a> {
    writer: Option<&'a mut HtmlWriter>,
    tag_name: EcoString,
}

impl<'a> GuardedTagWriter<'a> {
    pub(crate) fn new(writer: &'a mut HtmlWriter, tag_name: EcoString) -> Self {
        Self {
            writer: Some(writer),
            tag_name,
        }
    }

    fn writer_mut(&mut self) -> &mut HtmlWriter {
        self.writer
            .as_mut()
            .expect("GuardedTagWriter writer already taken")
    }

    pub(crate) fn write_attributes(&mut self, attributes: &[HtmlAttribute]) -> HtmlWriteResult<()> {
        for attr in attributes {
            self.writer_mut().attribute(&attr.name, &attr.value)?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn attribute_checked(&mut self, name: &str, value: &str) -> HtmlWriteResult<()> {
        let writer = self.writer_mut();
        if utils::is_safe_attribute_name(name) {
            writer.attribute(name, value)
        } else if writer.options.strict {
            Err(HtmlWriteError::InvalidHtmlAttribute(name.to_string()))
        } else {
            writer.emit_warning(format!(
                "Invalid attribute name '{name}' encountered. Skipping attribute in non-strict mode."
            ));
            Ok(())
        }
    }

    pub(crate) fn finish(mut self) -> HtmlWriteResult<GuardedTagBody<'a>> {
        self.writer_mut().finish_tag()?;
        let writer = self
            .writer
            .take()
            .expect("GuardedTagWriter writer already taken");
        Ok(GuardedTagBody::new(writer, self.tag_name))
    }

    pub(crate) fn finish_self_closing(mut self) -> HtmlWriteResult<&'a mut HtmlWriter> {
        self.writer_mut().finish_self_closing_tag()?;
        Ok(self
            .writer
            .take()
            .expect("GuardedTagWriter writer already taken"))
    }
}

pub(crate) struct GuardedTagBody<'a> {
    writer: Option<&'a mut HtmlWriter>,
    tag_name: EcoString,
}

impl<'a> GuardedTagBody<'a> {
    fn new(writer: &'a mut HtmlWriter, tag_name: EcoString) -> Self {
        Self {
            writer: Some(writer),
            tag_name,
        }
    }

    pub(crate) fn writer(&mut self) -> &mut HtmlWriter {
        self.writer
            .as_mut()
            .expect("GuardedTagBody writer already taken")
    }

    pub(crate) fn end(mut self) -> HtmlWriteResult<&'a mut HtmlWriter> {
        let writer = self
            .writer
            .as_mut()
            .expect("GuardedTagBody writer already taken");
        writer.end_tag(&self.tag_name)?;
        Ok(self
            .writer
            .take()
            .expect("GuardedTagBody writer already taken"))
    }
}
