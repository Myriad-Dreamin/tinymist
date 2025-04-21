//! LaTeX converter implementation

use std::fmt::Write;
use std::path::Path;

use base64::Engine;
use ecow::EcoString;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::ListState;
use crate::tags::md_tag;
use crate::tinymist_std::path::unix_slash;
use crate::Result;
use crate::TypliteFeat;

/// LaTeX converter implementation
#[derive(Clone)]
pub struct LaTeXConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
}

impl LaTeXConverter {
    pub fn convert(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        match root.tag {
            tag::head => Ok(()),
            tag::html | tag::body | md_tag::doc => self.convert_children(root, w),
            tag::span | tag::dl | tag::dt | tag::dd => {
                self.convert_children(root, w)?;
                Ok(())
            }
            tag::p => self.convert_paragraph(root, w),
            tag::ol => {
                let state = self.list_state;
                self.list_state = Some(ListState::Ordered);
                self.convert_children(root, w)?;
                self.list_state = state;
                Ok(())
            }
            tag::ul => {
                let state = self.list_state;
                self.list_state = Some(ListState::Unordered);
                self.convert_children(root, w)?;
                self.list_state = state;
                Ok(())
            }
            tag::li => {
                match self.list_state {
                    Some(ListState::Ordered) => w.push_str("1. "),
                    Some(ListState::Unordered) => w.push_str("- "),
                    None => {}
                }

                self.convert_children(root, w)?;
                w.push_str("\n");

                Ok(())
            }
            tag::figure => self.convert_children(root, w),
            tag::figcaption => Ok(()),
            md_tag::heading => self.convert_heading(root, w),
            md_tag::link => {
                let attrs = LinkAttr::parse(&root.attrs)?;

                w.push('[');
                self.convert_children(root, w)?;
                w.push(']');
                w.push('(');
                w.push_str(&attrs.dest);
                w.push(')');

                Ok(())
            }
            md_tag::parbreak => {
                w.push_str("\n\n");
                Ok(())
            }
            md_tag::linebreak => {
                w.push_str("\n");
                Ok(())
            }
            tag::strong | md_tag::strong => {
                w.push_str("**");
                self.convert_children(root, w)?;
                w.push_str("**");
                Ok(())
            }
            tag::em | md_tag::emph => {
                w.push_str("*");
                self.convert_children(root, w)?;
                w.push_str("*");
                Ok(())
            }
            md_tag::highlight => {
                w.push_str("==");
                self.convert_children(root, w)?;
                w.push_str("==");
                Ok(())
            }
            md_tag::strike => {
                w.push_str("~~");
                self.convert_children(root, w)?;
                w.push_str("~~");
                Ok(())
            }
            md_tag::raw => {
                let attrs = RawAttr::parse(&root.attrs)?;
                let lang = attrs.lang;
                let block = attrs.block;
                let text = attrs.text;
                let mut max_backticks = if block { 3 } else { 1 };
                let mut backticks = 0;
                for c in text.chars() {
                    if c == '`' {
                        max_backticks += 1;
                    } else {
                        max_backticks = backticks.max(max_backticks);
                        backticks = 0;
                    }
                }
                let backticks = "`".repeat(max_backticks);

                w.push_str(&backticks);
                if block {
                    w.push_str(&lang);
                    w.push('\n');
                }
                w.push_str(&text);
                if block {
                    w.push('\n');
                }
                w.push_str(&backticks);
                Ok(())
            }
            md_tag::label | md_tag::reference | md_tag::outline | md_tag::outline_entry => {
                w.push_str("`");
                self.convert_children(root, w)?;
                w.push_str("`");
                Ok(())
            }
            md_tag::quote => {
                w.push_str(">");
                self.convert_children(root, w)?;
                w.push_str("\n");
                Ok(())
            }
            md_tag::table | md_tag::grid | md_tag::table_cell | md_tag::grid_cell => {
                w.push_str("```");
                self.convert_children(root, w)?;
                w.push_str("```");
                Ok(())
            }
            md_tag::math_equation_inline | md_tag::math_equation_block => {
                w.push_str(
                    r#"\begin{equation}
\int x^2 \operatorname{d} x
\end{equation}
"#,
                );
                Ok(())
            }
            md_tag::image => {
                let attrs = ImageAttr::parse(&root.attrs)?;
                let src = unix_slash(Path::new(attrs.src.as_str()));

                write!(w, r#"![{}]({src})"#, attrs.alt)?;
                Ok(())
            }
            _ => panic!("unexpected tag: {:?}", root.tag),
        }
    }

    fn convert_children(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        for child in &root.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => self.write_frame(frame, w),
                HtmlNode::Text(text, _) => {
                    w.push_str(text);
                }
                HtmlNode::Element(element) => {
                    self.convert(element, w)?;
                }
            }
        }
        Ok(())
    }

    fn convert_heading(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let attrs = HeadingAttr::parse(&root.attrs)?;

        if attrs.level >= 4 || attrs.level == 0 {
            return Err(format!("heading level {} is not good", attrs.level).into());
        }

        w.push('\\');
        for _ in 0..(attrs.level - 1) {
            w.push_str("sub");
        }
        w.push_str("section{");

        self.convert_children(root, w)?;
        w.push('}');
        Ok(())
    }

    /// Encode a laid out frame into the writer.
    fn write_frame(&mut self, frame: &Frame, w: &mut EcoString) {
        // FIXME: This string replacement is obviously a hack.
        let svg = typst_svg::svg_frame(frame)
            .replace("<svg class", "<svg style=\"overflow: visible;\" class");

        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
        let _ = write!(
            w,
            r#"<img alt="typst-block" src="data:image/svg+xml;base64,{data}"/>"#
        );
    }

    fn convert_paragraph(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\n\n");
        self.convert_children(root, w)?;
        w.push_str("\n\n");
        Ok(())
    }
}
