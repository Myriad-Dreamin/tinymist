//! Core functionality for converting HTML to intermediate DocxNode structure

use std::path::Path;

use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::ListState;
use crate::tags::md_tag;
use crate::tinymist_std::path::unix_slash;
use crate::Result;
use crate::TypliteFeat;

use super::types::{DocxInline, DocxNode};
use super::utils::render_frame_to_png;
use super::writer::DocxWriter;

/// DOCX Converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    nodes: Vec<DocxNode>,
    inline_buffer: Vec<DocxInline>,
    list_numbering_ids: Vec<(bool, usize)>, // (is_ordered, numbering_id)
}

impl DocxConverter {
    /// Create a new DOCX converter
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            list_level: 0,
            nodes: Vec::new(),
            inline_buffer: Vec::new(),
            list_numbering_ids: Vec::new(),
        }
    }

    /// Convert HTML element to DOCX format
    pub fn convert(&mut self, root: &HtmlElement) -> Result<()> {
        self.nodes = Vec::new();
        self.inline_buffer = Vec::new();
        self.list_numbering_ids = Vec::new();
        self.convert_element(root)?;
        self.flush_inline_buffer();
        Ok(())
    }

    /// Export to DOCX byte data using a DocxWriter
    pub fn to_docx(&mut self) -> Result<Vec<u8>> {
        let mut writer = DocxWriter::new();
        writer.write(&self.nodes, &self.list_numbering_ids)
    }

    /// Flush inline buffer to paragraph
    fn flush_inline_buffer(&mut self) {
        if !self.inline_buffer.is_empty() {
            self.nodes.push(DocxNode::Paragraph {
                style: None,
                content: std::mem::take(&mut self.inline_buffer),
                numbering: None,
            });
        }
    }

    /// Flush inline buffer to styled paragraph
    fn flush_inline_buffer_as_styled_paragraph(&mut self, style: &str) {
        if !self.inline_buffer.is_empty() {
            self.nodes.push(DocxNode::Paragraph {
                style: Some(style.to_string()),
                content: std::mem::take(&mut self.inline_buffer),
                numbering: None,
            });
        }
    }

    /// Convert element to DocxNode structure
    fn convert_element(&mut self, element: &HtmlElement) -> Result<()> {
        match element.tag {
            tag::head => Ok(()),

            tag::html | tag::body | md_tag::doc => {
                for child in &element.children {
                    if let HtmlNode::Element(child_elem) = child {
                        self.convert_element(child_elem)?;
                    }
                }
                Ok(())
            }

            md_tag::parbreak => {
                self.flush_inline_buffer();
                Ok(())
            }

            md_tag::linebreak => {
                self.inline_buffer.push(DocxInline::LineBreak);
                Ok(())
            }

            md_tag::heading => self.convert_heading(element),

            tag::ol => self.convert_list(element, true),
            tag::ul => self.convert_list(element, false),

            tag::p | tag::span => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::dl | tag::dt | tag::dd => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::strong | md_tag::strong => self.convert_strong(element),
            tag::em | md_tag::emph => self.convert_emphasis(element),
            md_tag::highlight => self.convert_highlight(element),
            md_tag::strike => self.convert_strike(element),

            md_tag::raw => self.convert_raw(element),

            md_tag::label | md_tag::reference | md_tag::outline | md_tag::outline_entry => {
                self.convert_inline_code(element)
            }

            md_tag::quote => self.convert_quote(element),

            md_tag::table | md_tag::grid => self.convert_table(element),

            md_tag::link => self.convert_link(element),

            md_tag::image => self.convert_image(element),

            md_tag::math_equation_inline => self.convert_math(element, false),
            md_tag::math_equation_block => self.convert_math(element, true),

            tag::div | tag::figure => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::figcaption => self.convert_block(element, "Caption"),

            tag::pre => self.convert_block(element, "CodeBlock"),

            _ => {
                self.convert_children(element)?;
                Ok(())
            }
        }
    }

    /// Convert children elements
    fn convert_children(&mut self, element: &HtmlElement) -> Result<()> {
        for child in &element.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => {
                    self.convert_frame(frame)?;
                }
                HtmlNode::Text(text, _) => {
                    self.inline_buffer.push(DocxInline::Text(text.to_string()));
                }
                HtmlNode::Element(element) => {
                    self.convert_element(element)?;
                }
            }
        }
        Ok(())
    }

    /// Convert children into target buffer
    fn convert_children_into(
        &mut self,
        target: &mut Vec<DocxInline>,
        element: &HtmlElement,
    ) -> Result<()> {
        let prev_buffer = std::mem::take(&mut self.inline_buffer);
        self.convert_children(element)?;
        target.append(&mut self.inline_buffer);
        self.inline_buffer = prev_buffer;
        Ok(())
    }

    /// Convert frame to image
    fn convert_frame(&mut self, frame: &Frame) -> Result<()> {
        let png_data = render_frame_to_png(frame)?;
        self.inline_buffer
            .push(DocxInline::Image { data: png_data });
        Ok(())
    }

    /// Convert heading
    fn convert_heading(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        let attrs = HeadingAttr::parse(&element.attrs)?;
        if attrs.level >= 7 {
            return Err(format!("heading level {} is too high", attrs.level).into());
        }

        let style_name = match attrs.level {
            1 => "Heading1",
            2 => "Heading2",
            3 => "Heading3",
            4 => "Heading4",
            5 => "Heading5",
            _ => "Heading6",
        };

        self.convert_children(element)?;
        self.flush_inline_buffer_as_styled_paragraph(style_name);

        Ok(())
    }

    /// Convert list item
    fn convert_list_item(&mut self, element: &HtmlElement, numbering_id: usize) -> Result<()> {
        let mut main_content = Vec::new();
        let mut nested_nodes = Vec::new();
        let level = self.list_level.saturating_sub(1);

        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => {
                    main_content.push(DocxInline::Text(text.to_string()));
                }
                HtmlNode::Element(child_elem) => {
                    if child_elem.tag == tag::ul || child_elem.tag == tag::ol {
                        if !main_content.is_empty() {
                            self.nodes.push(DocxNode::Paragraph {
                                style: None,
                                content: std::mem::take(&mut main_content),
                                numbering: Some((numbering_id, level)),
                            });
                        }

                        let prev_nodes = std::mem::take(&mut self.nodes);
                        let prev_buffer = std::mem::take(&mut self.inline_buffer);

                        self.convert_element(child_elem)?;
                        self.flush_inline_buffer();

                        nested_nodes = std::mem::take(&mut self.nodes);

                        self.nodes = prev_nodes;
                        self.inline_buffer = prev_buffer;
                    } else {
                        let prev_buffer = std::mem::take(&mut self.inline_buffer);
                        self.convert_element(child_elem)?;
                        main_content.append(&mut self.inline_buffer);
                        self.inline_buffer = prev_buffer;
                    }
                }
                _ => {}
            }
        }

        if !main_content.is_empty() {
            self.nodes.push(DocxNode::Paragraph {
                style: None,
                content: main_content,
                numbering: Some((numbering_id, level)),
            });
        } else if nested_nodes.is_empty() {
            self.nodes.push(DocxNode::Paragraph {
                style: None,
                content: vec![DocxInline::Text("".to_string())],
                numbering: Some((numbering_id, level)),
            });
        }

        // 添加嵌套内容
        self.nodes.extend(nested_nodes);

        Ok(())
    }

    /// Convert list (common implementation for ordered and unordered)
    fn convert_list(&mut self, element: &HtmlElement, is_ordered: bool) -> Result<()> {
        self.flush_inline_buffer();

        let prev_state = self.list_state;
        let prev_level = self.list_level;

        let numbering_id = self.create_list_numbering(is_ordered);

        self.list_state = Some(if is_ordered {
            ListState::Ordered
        } else {
            ListState::Unordered
        });
        self.list_level = prev_level + 1;

        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    self.convert_list_item(li, numbering_id)?;
                }
            }
        }

        self.list_level = prev_level;
        self.list_state = prev_state;

        Ok(())
    }

    /// Create unique list numbering
    fn create_list_numbering(&mut self, is_ordered: bool) -> usize {
        let numbering_id = self.list_numbering_ids.len() + 1;
        self.list_numbering_ids.push((is_ordered, numbering_id));
        numbering_id
    }

    /// Convert strong/bold text
    fn convert_strong(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(DocxInline::Strong(content));
        Ok(())
    }

    /// Convert emphasis/italic text
    fn convert_emphasis(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(DocxInline::Emphasis(content));
        Ok(())
    }

    /// Convert highlighted text
    fn convert_highlight(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(DocxInline::Highlight(content));
        Ok(())
    }

    /// Convert strikethrough text
    fn convert_strike(&mut self, element: &HtmlElement) -> Result<()> {
        let mut content = Vec::new();
        self.convert_children_into(&mut content, element)?;
        self.inline_buffer.push(DocxInline::Strike(content));
        Ok(())
    }

    /// Convert raw code
    fn convert_raw(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = RawAttr::parse(&element.attrs)?;

        if attrs.block {
            self.flush_inline_buffer();

            // Add language paragraph
            self.inline_buffer
                .push(DocxInline::Text(attrs.lang.to_string()));
            self.flush_inline_buffer_as_styled_paragraph("CodeBlock");

            // 逐行处理代码，确保保留换行
            let lines: Vec<&str> = attrs.text.split('\n').collect();

            // 对每行代码单独创建一个段落，保留换行
            for line in lines {
                self.inline_buffer.push(DocxInline::Text(line.to_string()));
                self.flush_inline_buffer_as_styled_paragraph("CodeBlock");
            }
        } else {
            self.inline_buffer
                .push(DocxInline::Code(attrs.text.to_string()));
        }

        Ok(())
    }

    /// Convert inline code
    fn convert_inline_code(&mut self, element: &HtmlElement) -> Result<()> {
        let mut text = String::new();

        for child in &element.children {
            if let HtmlNode::Text(content, _) = child {
                text.push_str(content);
            }
        }

        self.inline_buffer.push(DocxInline::Code(text));
        Ok(())
    }

    /// Convert blockquote
    fn convert_quote(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        self.convert_children(element)?;
        self.flush_inline_buffer_as_styled_paragraph("Blockquote");

        Ok(())
    }

    /// Convert link
    fn convert_link(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&element.attrs)?;
        let mut content = Vec::new();

        self.convert_children_into(&mut content, element)?;

        self.inline_buffer.push(DocxInline::Hyperlink {
            url: attrs.dest.to_string(),
            content,
        });

        Ok(())
    }

    /// Convert image
    fn convert_image(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        let src = unix_slash(Path::new(attrs.src.as_str()));

        self.flush_inline_buffer();

        if let Ok(img_data) = std::fs::read(&src) {
            self.nodes.push(DocxNode::Image {
                data: img_data,
                alt: attrs.alt.to_string(),
            });
        } else {
            let placeholder = format!("[Image: {}]", attrs.alt);
            self.inline_buffer.push(DocxInline::Text(placeholder));
            self.flush_inline_buffer();
        }

        Ok(())
    }

    /// Common implementation for math conversion
    fn convert_math(&mut self, element: &HtmlElement, is_block: bool) -> Result<()> {
        if is_block {
            self.flush_inline_buffer();
        }

        // 尝试查找 Frame 元素
        let maybe_frame = element.children.iter().find_map(|child| {
            if let HtmlNode::Frame(frame) = child {
                Some(frame)
            } else {
                None
            }
        });

        // 处理数学表达式
        if let Some(frame) = maybe_frame {
            // 渲染数学表达式为 PNG
            match render_frame_to_png(frame) {
                Ok(png_data) => {
                    let image_content = DocxInline::Image { data: png_data };

                    // 根据是否为块级元素决定如何处理
                    if is_block {
                        self.nodes.push(DocxNode::Paragraph {
                            style: Some("MathBlock".to_string()),
                            content: vec![image_content],
                            numbering: None,
                        });
                    } else {
                        self.inline_buffer.push(image_content);
                    }
                }
                Err(_) => {
                    // 渲染失败时使用占位符文本
                    let fallback = DocxInline::Text("[Math Expression]".to_string());
                    if is_block {
                        self.nodes.push(DocxNode::Paragraph {
                            style: Some("MathBlock".to_string()),
                            content: vec![fallback],
                            numbering: None,
                        });
                    } else {
                        self.inline_buffer.push(fallback);
                    }
                }
            }
        } else {
            // 没有找到 Frame 元素，使用占位符文本
            let fallback = DocxInline::Text("[Math Expression]".to_string());
            if is_block {
                self.nodes.push(DocxNode::Paragraph {
                    style: Some("MathBlock".to_string()),
                    content: vec![fallback],
                    numbering: None,
                });
            } else {
                self.inline_buffer.push(fallback);
            }
        }

        Ok(())
    }

    /// Convert table
    fn convert_table(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        let mut rows = Vec::new();

        // Find real table element - either directly in m1table or inside m1grid/m1table
        let real_table_elem = if element.tag == md_tag::grid {
            // For grid: grid -> table -> table
            let mut inner_table = None;

            for child in &element.children {
                if let HtmlNode::Element(table_elem) = child {
                    if table_elem.tag == md_tag::table {
                        // Find table tag inside m1table
                        for inner_child in &table_elem.children {
                            if let HtmlNode::Element(inner) = inner_child {
                                if inner.tag == tag::table {
                                    inner_table = Some(inner);
                                    break;
                                }
                            }
                        }

                        if inner_table.is_some() {
                            break;
                        }
                    }
                }
            }

            inner_table
        } else {
            // For m1table -> table
            let mut direct_table = None;

            for child in &element.children {
                if let HtmlNode::Element(table_elem) = child {
                    if table_elem.tag == tag::table {
                        direct_table = Some(table_elem);
                        break;
                    }
                }
            }

            direct_table
        };

        // Process table rows and cells if the real table element was found
        if let Some(table) = real_table_elem {
            // Process rows in table
            for row_node in &table.children {
                if let HtmlNode::Element(row_elem) = row_node {
                    if row_elem.tag == tag::tr {
                        let mut cells = Vec::new();

                        // Process cells in this row
                        for cell_node in &row_elem.children {
                            if let HtmlNode::Element(cell_elem) = cell_node {
                                if cell_elem.tag == tag::td {
                                    let prev_nodes = std::mem::take(&mut self.nodes);
                                    let prev_buffer = std::mem::take(&mut self.inline_buffer);

                                    self.convert_children(cell_elem)?;
                                    self.flush_inline_buffer();

                                    let cell_content = std::mem::take(&mut self.nodes);
                                    cells.push(cell_content);

                                    self.nodes = prev_nodes;
                                    self.inline_buffer = prev_buffer;
                                }
                            }
                        }

                        if !cells.is_empty() {
                            rows.push(cells);
                        }
                    }
                }
            }
        }

        if !rows.is_empty() {
            self.nodes.push(DocxNode::Table { rows });
        }

        Ok(())
    }

    /// Convert block element
    fn convert_block(&mut self, element: &HtmlElement, style: &str) -> Result<()> {
        self.flush_inline_buffer();
        self.convert_children(element)?;
        self.flush_inline_buffer_as_styled_paragraph(style);
        Ok(())
    }
}
