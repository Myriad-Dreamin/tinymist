//! DOCX converter implementation using docx-rs

use std::path::Path;

use docx_rs::*;
use image::GenericImageView;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::ListState;
use crate::tags::md_tag;
use crate::tinymist_std::path::unix_slash;
use crate::Result;
use crate::TypliteFeat;
use resvg::tiny_skia::{self, Pixmap};
use resvg::usvg::{Options, Tree};

/// Get image dimensions
fn get_image_size(img_data: &[u8]) -> Option<(u32, u32)> {
    match image::load_from_memory(img_data) {
        Ok(img) => {
            let (width, height) = img.dimensions();
            Some((width, height))
        }
        Err(_) => None,
    }
}

/// Document style management
#[derive(Clone, Debug)]
struct DocxStyles {
    initialized: bool,
}

impl DocxStyles {
    fn new() -> Self {
        Self { initialized: false }
    }

    fn create_heading_style(name: &str, display_name: &str, size: usize) -> Style {
        Style::new(name, StyleType::Paragraph)
            .name(display_name)
            .size(size)
            .bold()
    }

    fn initialize_styles(&self, docx: Docx) -> Docx {
        if self.initialized {
            return docx;
        }

        let heading1 = Self::create_heading_style("Heading1", "Heading 1", 32);
        let heading2 = Self::create_heading_style("Heading2", "Heading 2", 28);
        let heading3 = Self::create_heading_style("Heading3", "Heading 3", 26);
        let heading4 = Self::create_heading_style("Heading4", "Heading 4", 24);
        let heading5 = Self::create_heading_style("Heading5", "Heading 5", 22);
        let heading6 = Self::create_heading_style("Heading6", "Heading 6", 20);

        let courier_fonts = RunFonts::new()
            .ascii("Courier New")
            .hi_ansi("Courier New")
            .east_asia("Courier New")
            .cs("Courier New");

        let code_block = Style::new("CodeBlock", StyleType::Paragraph)
            .name("Code Block")
            .fonts(courier_fonts.clone())
            .size(18);

        let code_inline = Style::new("CodeInline", StyleType::Character)
            .name("Code Inline")
            .fonts(courier_fonts)
            .size(18);

        let math_block = Style::new("MathBlock", StyleType::Paragraph)
            .name("Math Block")
            .align(AlignmentType::Center);

        let emphasis = Style::new("Emphasis", StyleType::Character)
            .name("Emphasis")
            .italic();

        let strong = Style::new("Strong", StyleType::Character)
            .name("Strong")
            .bold();

        let highlight = Style::new("Highlight", StyleType::Character)
            .name("Highlight")
            .highlight("yellow");

        let hyperlink = Style::new("Hyperlink", StyleType::Character)
            .name("Hyperlink")
            .color("0000FF")
            .underline("single");

        let blockquote = Style::new("Blockquote", StyleType::Paragraph)
            .name("Block Quote")
            .indent(Some(720), None, None, None)
            .italic();

        let caption = Style::new("Caption", StyleType::Paragraph)
            .name("Caption")
            .italic()
            .size(16)
            .align(AlignmentType::Center);

        let table = Style::new("Table", StyleType::Table)
            .name("Table")
            .table_align(TableAlignmentType::Center);

        docx.add_style(heading1)
            .add_style(heading2)
            .add_style(heading3)
            .add_style(heading4)
            .add_style(heading5)
            .add_style(heading6)
            .add_style(code_block)
            .add_style(code_inline)
            .add_style(math_block)
            .add_style(emphasis)
            .add_style(strong)
            .add_style(highlight)
            .add_style(hyperlink)
            .add_style(blockquote)
            .add_style(caption)
            .add_style(table)
    }
}

/// List numbering management
#[derive(Clone, Debug)]
struct DocxNumbering {
    initialized: bool,
    next_id: usize,
}

impl DocxNumbering {
    fn new() -> Self {
        Self {
            initialized: false,
            next_id: 1,
        }
    }

    fn create_list_level(id: usize, format: &str, text: &str, is_bullet: bool) -> Level {
        let indent_size = 720 * (id + 1) as i32;
        let hanging_indent = if is_bullet { 360 } else { 420 };

        Level::new(
            id,
            Start::new(1),
            NumberFormat::new(format),
            LevelText::new(text),
            LevelJc::new("left"),
        )
        .indent(
            Some(indent_size),
            Some(SpecialIndentType::Hanging(hanging_indent)),
            None,
            None,
        )
    }

    fn initialize_numbering(&mut self, docx: Docx) -> Docx {
        if self.initialized {
            return docx;
        }

        self.initialized = true;
        docx
    }

    /// Create a new ordered list numbering, including a new AbstractNumbering instance
    fn create_ordered_numbering(&mut self, docx: Docx) -> (Docx, usize) {
        let abstract_id = self.next_id;
        let numbering_id = self.next_id;
        self.next_id += 1;

        let mut ordered_abstract = AbstractNumbering::new(abstract_id);

        for i in 0..9 {
            let level_text = match i {
                0 => "%1.",
                1 => "%2.",
                2 => "%3.",
                3 => "%4.",
                4 => "%5.",
                5 => "%6.",
                _ => "%7.",
            };

            let number_format = match i {
                0 => "decimal",
                1 => "lowerLetter",
                2 => "lowerRoman",
                3 => "upperRoman",
                4 => "decimal",
                5 => "lowerLetter",
                _ => "decimal",
            };

            let mut ordered_level = Self::create_list_level(i, number_format, level_text, false);

            if i > 0 {
                ordered_level = ordered_level.level_restart(0_u32);
            }

            ordered_abstract = ordered_abstract.add_level(ordered_level);
        }

        let docx = docx
            .add_abstract_numbering(ordered_abstract)
            .add_numbering(Numbering::new(numbering_id, abstract_id));

        (docx, numbering_id)
    }

    /// Create a new unordered list numbering, including a new AbstractNumbering instance
    fn create_unordered_numbering(&mut self, docx: Docx) -> (Docx, usize) {
        let abstract_id = self.next_id;
        let numbering_id = self.next_id;
        self.next_id += 1;

        // Create AbstractNumbering for unordered list
        let mut unordered_abstract = AbstractNumbering::new(abstract_id);

        // Add 9 levels of definition
        for i in 0..9 {
            let bullet_text = match i {
                0 => "•",
                1 => "○",
                2 => "▪",
                3 => "▫",
                4 => "◆",
                _ => "◇",
            };

            let unordered_level = Self::create_list_level(i, "bullet", bullet_text, true);
            unordered_abstract = unordered_abstract.add_level(unordered_level);
        }

        let docx = docx
            .add_abstract_numbering(unordered_abstract)
            .add_numbering(Numbering::new(numbering_id, abstract_id));

        (docx, numbering_id)
    }
}

/// Document structure representation before converting to DOCX
#[derive(Clone, Debug)]
enum DocxNode {
    Paragraph {
        style: Option<String>,
        content: Vec<DocxInline>,
        numbering: Option<(usize, usize)>, // numbering_id, level
    },
    Table {
        rows: Vec<Vec<Vec<DocxNode>>>, // rows, cells, content_nodes
    },
    Image {
        data: Vec<u8>,
        alt: String,
    },
}

/// Inline content representation
#[derive(Clone, Debug)]
enum DocxInline {
    Text(String),
    Strong(Vec<DocxInline>),
    Emphasis(Vec<DocxInline>),
    Highlight(Vec<DocxInline>),
    Strike(Vec<DocxInline>),
    Code(String),
    Hyperlink {
        url: String,
        content: Vec<DocxInline>,
    },
    Image {
        data: Vec<u8>,
        alt: String,
    },
    LineBreak,
}

/// DOCX Converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    styles: DocxStyles,
    numbering: DocxNumbering,
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
            styles: DocxStyles::new(),
            numbering: DocxNumbering::new(),
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

            md_tag::math_equation_inline => self.convert_math_inline(element),
            md_tag::math_equation_block => self.convert_math_block(element),

            tag::div | tag::figure => {
                self.convert_children(element)?;
                Ok(())
            }

            tag::figcaption => self.convert_caption(element),

            tag::pre => self.convert_pre(element),

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
        let png_data = self.render_frame_to_png(frame)?;
        self.inline_buffer.push(DocxInline::Image {
            data: png_data,
            alt: "typst-block".to_string(),
        });
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
        // 先创建列表项的起始段落，带有编号
        let mut content_elements = vec![DocxNode::Paragraph {
            style: None,
            content: Vec::new(), // 先创建空内容，后面会填充
            numbering: Some((numbering_id, self.list_level.saturating_sub(1))),
        }];

        // 用于收集列表项内所有内容
        let mut item_texts = Vec::new();
        let mut nested_nodes = Vec::new();

        // 处理列表项的所有内容
        for child in &element.children {
            match child {
                HtmlNode::Text(text, _) => {
                    item_texts.push(DocxInline::Text(text.to_string()));
                }
                HtmlNode::Element(child_elem) => {
                    if child_elem.tag == tag::ul || child_elem.tag == tag::ol {
                        // 遇到嵌套列表，先保存当前位置，处理完后再恢复
                        let prev_nodes = std::mem::take(&mut self.nodes);
                        let prev_buffer = std::mem::take(&mut self.inline_buffer);

                        // 处理嵌套列表
                        self.convert_element(child_elem)?;
                        self.flush_inline_buffer();

                        // 保存嵌套列表的节点
                        nested_nodes.extend(std::mem::take(&mut self.nodes));

                        // 恢复状态
                        self.nodes = prev_nodes;
                        self.inline_buffer = prev_buffer;
                    } else {
                        // 处理其他内联元素
                        let prev_buffer = std::mem::take(&mut self.inline_buffer);
                        self.convert_element(child_elem)?;
                        item_texts.append(&mut self.inline_buffer);
                        self.inline_buffer = prev_buffer;
                    }
                }
                _ => {}
            }
        }

        // 将收集的文本内容填充到第一个段落中
        if let DocxNode::Paragraph { content, .. } = &mut content_elements[0] {
            *content = item_texts;
        }

        // 添加嵌套内容
        content_elements.extend(nested_nodes);

        // 将所有内容添加到文档结构中
        self.nodes.extend(content_elements);

        Ok(())
    }

    /// Convert list (common implementation for ordered and unordered)
    fn convert_list(&mut self, element: &HtmlElement, is_ordered: bool) -> Result<()> {
        self.flush_inline_buffer();

        let prev_state = self.list_state;
        let prev_level = self.list_level;

        // 为每个新列表创建唯一的编号 ID
        let numbering_id = self.create_list_numbering(is_ordered);

        // 设置当前列表状态
        self.list_state = Some(if is_ordered {
            ListState::Ordered
        } else {
            ListState::Unordered
        });
        self.list_level = prev_level + 1;

        // 处理列表项
        for child in &element.children {
            if let HtmlNode::Element(li) = child {
                if li.tag == tag::li {
                    self.convert_list_item(li, numbering_id)?;
                }
            }
        }

        // 恢复列表状态
        self.list_level = prev_level;
        self.list_state = prev_state;

        Ok(())
    }

    /// 为每个列表创建唯一的编号 ID
    fn create_list_numbering(&mut self, is_ordered: bool) -> usize {
        let (_, numbering_id) = if is_ordered {
            self.numbering.create_ordered_numbering(Docx::new())
        } else {
            self.numbering.create_unordered_numbering(Docx::new())
        };

        // 保存创建的编号 ID 以便后续使用
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

    /// Convert inline math
    fn convert_math_inline(&mut self, element: &HtmlElement) -> Result<()> {
        self.convert_math(element, false)
    }

    /// Convert block math
    fn convert_math_block(&mut self, element: &HtmlElement) -> Result<()> {
        self.convert_math(element, true)
    }

    /// Common implementation for math conversion
    fn convert_math(&mut self, element: &HtmlElement, is_block: bool) -> Result<()> {
        if is_block {
            self.flush_inline_buffer();
        }

        // Find Frame child element
        let maybe_frame = element.children.iter().find_map(|child| {
            if let HtmlNode::Frame(frame) = child {
                Some(frame)
            } else {
                None
            }
        });

        if let Some(frame) = maybe_frame {
            let png_data = self.render_frame_to_png(frame)?;

            if is_block {
                self.nodes.push(DocxNode::Paragraph {
                    style: Some("MathBlock".to_string()),
                    content: vec![DocxInline::Image {
                        data: png_data,
                        alt: "Math Expression".to_string(),
                    }],
                    numbering: None,
                });
            } else {
                self.inline_buffer.push(DocxInline::Image {
                    data: png_data,
                    alt: "Math Expression".to_string(),
                });
            }
        } else if is_block {
            self.nodes.push(DocxNode::Paragraph {
                style: Some("MathBlock".to_string()),
                content: vec![DocxInline::Text("[Math Expression]".to_string())],
                numbering: None,
            });
        } else {
            self.inline_buffer
                .push(DocxInline::Text("[Math Expression]".to_string()));
        }

        Ok(())
    }

    /// Convert table
    fn convert_table(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();

        let mut rows = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(row_elem) = child {
                let mut cells = Vec::new();

                for cell_node in &row_elem.children {
                    if let HtmlNode::Element(cell_elem) = cell_node {
                        if cell_elem.tag == md_tag::table_cell || cell_elem.tag == md_tag::grid_cell
                        {
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

        if !rows.is_empty() {
            self.nodes.push(DocxNode::Table { rows });
        }

        Ok(())
    }

    /// Convert figure caption
    fn convert_caption(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();
        self.convert_children(element)?;
        self.flush_inline_buffer_as_styled_paragraph("Caption");
        Ok(())
    }

    /// Convert pre element (preformatted text)
    fn convert_pre(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_inline_buffer();
        self.convert_children(element)?;
        self.flush_inline_buffer_as_styled_paragraph("CodeBlock");
        Ok(())
    }

    /// Render frame to PNG image
    fn render_frame_to_png(&self, frame: &Frame) -> Result<Vec<u8>> {
        let svg = typst_svg::svg_frame(frame);

        let dpi = 300.0;
        let scale_factor = dpi / 96.0;

        let opt = Options {
            dpi,
            ..Options::default()
        };

        let rtree = match Tree::from_str(&svg, &opt) {
            Ok(tree) => tree,
            Err(e) => return Err(format!("SVG parse error: {:?}", e).into()),
        };

        let size = rtree.size().to_int_size();
        let width = (size.width() as f32 * scale_factor) as u32;
        let height = (size.height() as f32 * scale_factor) as u32;

        let mut pixmap = Pixmap::new(width, height).ok_or("Failed to create pixmap")?;

        resvg::render(
            &rtree,
            tiny_skia::Transform::from_scale(scale_factor, scale_factor),
            &mut pixmap.as_mut(),
        );

        pixmap
            .encode_png()
            .map_err(|e| format!("PNG encode error: {:?}", e).into())
    }

    /// Calculate image dimensions
    fn calculate_image_dimensions(&self, img_data: &[u8], scale_factor: Option<f32>) -> (u32, u32) {
        let actual_scale = scale_factor.unwrap_or(1.0);

        if let Some((w, h)) = get_image_size(img_data) {
            let max_width = 5486400;
            let scaled_w = (w as f32 * actual_scale) as u32;
            let scaled_h = (h as f32 * actual_scale) as u32;

            if scaled_w > max_width {
                let ratio = scaled_h as f32 / scaled_w as f32;
                let new_width = max_width;
                let new_height = (max_width as f32 * ratio) as u32;
                (new_width, new_height)
            } else {
                (scaled_w * 9525, scaled_h * 9525)
            }
        } else {
            (4000000, 3000000)
        }
    }

    /// Export to DOCX byte data
    pub fn to_docx(&mut self) -> Result<Vec<u8>> {
        let mut docx = Docx::new();
        docx = self.styles.initialize_styles(docx);

        // 创建所有需要的编号定义
        let mut numbering = DocxNumbering::new();
        for (is_ordered, _) in &self.list_numbering_ids {
            let (new_docx, _) = if *is_ordered {
                numbering.create_ordered_numbering(docx)
            } else {
                numbering.create_unordered_numbering(docx)
            };
            docx = new_docx;
        }

        // 初始化编号
        docx = numbering.initialize_numbering(docx);

        // Process all nodes to create the final DOCX document
        for node in &self.nodes {
            docx = match node {
                DocxNode::Paragraph {
                    style,
                    content,
                    numbering,
                } => {
                    let mut para = Paragraph::new();

                    // Apply style if specified
                    if let Some(style_name) = style {
                        para = para.style(style_name);
                    }

                    // Apply numbering if specified
                    if let Some((numbering_id, level)) = numbering {
                        para = para
                            .numbering(NumberingId::new(*numbering_id), IndentLevel::new(*level));
                    }

                    // Process content
                    para = self.process_inline_content(para, content);

                    // Only add paragraph if it has content
                    if !para.children.is_empty() {
                        docx.add_paragraph(para)
                    } else {
                        docx
                    }
                }
                DocxNode::Table { rows } => {
                    let mut table = Table::new(vec![]).style("Table");

                    for row_cells in rows {
                        let mut cells = Vec::new();

                        for cell_nodes in row_cells {
                            let mut table_cell = TableCell::new();

                            for cell_node in cell_nodes {
                                match cell_node {
                                    DocxNode::Paragraph {
                                        style,
                                        content,
                                        numbering,
                                    } => {
                                        let mut para = Paragraph::new();

                                        if let Some(style_name) = style {
                                            para = para.style(style_name);
                                        }

                                        if let Some((numbering_id, level)) = numbering {
                                            para = para.numbering(
                                                NumberingId::new(*numbering_id),
                                                IndentLevel::new(*level),
                                            );
                                        }

                                        para = self.process_inline_content(para, content);

                                        if !para.children.is_empty() {
                                            table_cell = table_cell.add_paragraph(para);
                                        }
                                    }
                                    _ => {} // 表格中只处理段落类型
                                }
                            }

                            cells.push(table_cell);
                        }

                        let table_row = TableRow::new(cells).cant_split();

                        table = table.add_row(table_row);
                    }

                    docx.add_table(table)
                }
                DocxNode::Image { data, alt } => {
                    let (width, height) = self.calculate_image_dimensions(data, None);
                    let pic = Pic::new(data).size(width, height);

                    let img_para = Paragraph::new().add_run(Run::new().add_image(pic));
                    let doc_with_img = docx.add_paragraph(img_para);

                    if !alt.is_empty() {
                        let caption_para = Paragraph::new()
                            .style("Caption")
                            .add_run(Run::new().add_text(alt));
                        doc_with_img.add_paragraph(caption_para)
                    } else {
                        doc_with_img
                    }
                }
            };
        }

        // Build the document and return the bytes
        let docx_built = docx.build();
        let mut buffer = Vec::new();
        docx_built
            .pack(&mut std::io::Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;

        Ok(buffer)
    }

    /// Process inline content to build a paragraph
    fn process_inline_content(&self, mut para: Paragraph, content: &[DocxInline]) -> Paragraph {
        for inline in content {
            match inline {
                DocxInline::Text(text) => {
                    if !text.is_empty() {
                        para = para.add_run(Run::new().add_text(text));
                    }
                }
                DocxInline::Strong(nested) => {
                    let mut strong_run = Run::new().style("Strong");
                    strong_run = self.process_inline_run(strong_run, nested);
                    if !strong_run.children.is_empty() {
                        para = para.add_run(strong_run);
                    }
                }
                DocxInline::Emphasis(nested) => {
                    let mut emph_run = Run::new().style("Emphasis");
                    emph_run = self.process_inline_run(emph_run, nested);
                    if !emph_run.children.is_empty() {
                        para = para.add_run(emph_run);
                    }
                }
                DocxInline::Highlight(nested) => {
                    let mut highlight_run = Run::new().style("Highlight");
                    highlight_run = self.process_inline_run(highlight_run, nested);
                    if !highlight_run.children.is_empty() {
                        para = para.add_run(highlight_run);
                    }
                }
                DocxInline::Strike(nested) => {
                    let mut strike_run = Run::new().strike();
                    strike_run = self.process_inline_run(strike_run, nested);
                    if !strike_run.children.is_empty() {
                        para = para.add_run(strike_run);
                    }
                }
                DocxInline::Code(text) => {
                    if !text.is_empty() {
                        para = para.add_run(Run::new().style("CodeInline").add_text(text));
                    }
                }
                DocxInline::Hyperlink {
                    url,
                    content: nested,
                } => {
                    let mut hyperlink_run = Run::new().style("Hyperlink");
                    hyperlink_run = self.process_inline_run(hyperlink_run, nested);
                    if !hyperlink_run.children.is_empty() {
                        let hyperlink =
                            Hyperlink::new(url, HyperlinkType::External).add_run(hyperlink_run);
                        para = para.add_hyperlink(hyperlink);
                    }
                }
                DocxInline::Image { data, alt } => {
                    let (width, height) =
                        self.calculate_image_dimensions(data, Some(96.0 / 300.0 / 2.0));
                    let pic = Pic::new(data).size(width, height);
                    para = para.add_run(Run::new().add_image(pic));
                }
                DocxInline::LineBreak => {
                    para = para.add_run(Run::new().add_break(BreakType::TextWrapping));
                }
            }
        }

        para
    }

    /// Process inline content to build a run
    fn process_inline_run(&self, mut run: Run, content: &[DocxInline]) -> Run {
        for inline in content {
            match inline {
                DocxInline::Text(text) => {
                    if !text.is_empty() {
                        run = run.add_text(text);
                    }
                }
                DocxInline::LineBreak => {
                    run = run.add_break(BreakType::TextWrapping);
                }
                DocxInline::Image { data, .. } => {
                    let (width, height) =
                        self.calculate_image_dimensions(data, Some(96.0 / 300.0 / 2.0));
                    let pic = Pic::new(data).size(width, height);
                    run = run.add_image(pic);
                }
                // 其他内联类型在嵌套情况下会添加为文本
                _ => {
                    // 简单处理，将复杂的嵌套元素转换为文本
                    run = run.add_text("[Complex nested content]");
                }
            }
        }

        run
    }
}
