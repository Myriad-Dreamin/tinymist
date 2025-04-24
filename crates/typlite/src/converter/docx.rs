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

fn get_image_size(img_data: &[u8]) -> Option<(u32, u32)> {
    match image::load_from_memory(img_data) {
        Ok(img) => {
            let (width, height) = img.dimensions();
            Some((width, height))
        }
        Err(_) => None,
    }
}

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

#[derive(Clone, Debug)]
struct DocxNumbering {
    initialized: bool,
}

impl DocxNumbering {
    fn new() -> Self {
        Self { initialized: false }
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

        let mut ordered_abstract = AbstractNumbering::new(1);
        let mut unordered_abstract = AbstractNumbering::new(2);

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
                ordered_level = ordered_level.level_restart(0 as u32);
            }

            ordered_abstract = ordered_abstract.add_level(ordered_level);

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
            .add_abstract_numbering(ordered_abstract)
            .add_abstract_numbering(unordered_abstract);

        let ordered_numbering = Numbering::new(1, 1);
        let unordered_numbering = Numbering::new(2, 2);

        self.initialized = true;

        docx.add_numbering(ordered_numbering)
            .add_numbering(unordered_numbering)
    }
}

#[derive(Debug, Clone)]
struct ContentBuilder {
    current_paragraph: Option<Paragraph>,
    current_run: Option<Run>,
    text_buffer: String,
}

impl ContentBuilder {
    fn new() -> Self {
        Self {
            current_paragraph: Some(Paragraph::new()),
            current_run: Some(Run::new()),
            text_buffer: String::new(),
        }
    }

    fn flush_run(&mut self) -> Result<()> {
        if !self.text_buffer.is_empty() {
            if let Some(ref mut run) = self.current_run {
                *run = run.clone().add_text(&self.text_buffer);
            }

            self.text_buffer.clear();
        }

        if let (Some(ref mut para), Some(run)) =
            (&mut self.current_paragraph, self.current_run.take())
        {
            *para = para.clone().add_run(run);
        }

        self.current_run = Some(Run::new());

        Ok(())
    }

    fn add_text(&mut self, text: &str) {
        self.text_buffer.push_str(text);
    }

    fn add_line_break(&mut self) {
        self.flush_run().ok();
        self.current_run
            .as_mut()
            .map(|run| run.clone().add_break(BreakType::TextWrapping));
    }

    fn take_paragraph(&mut self) -> Option<Paragraph> {
        self.flush_run().ok()?;
        self.current_paragraph.take()
    }

    fn set_paragraph(&mut self, paragraph: Paragraph) {
        self.current_paragraph = Some(paragraph);
    }

    fn set_run(&mut self, run: Run) {
        self.current_run = Some(run);
    }

    fn take_run(&mut self) -> Option<Run> {
        self.current_run.take()
    }

    fn clear_buffer(&mut self) {
        self.text_buffer.clear();
    }

    fn get_buffer_clone(&self) -> String {
        self.text_buffer.clone()
    }
}

#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    docx: Docx,
    content_builder: ContentBuilder,
    styles: DocxStyles,
    numbering: DocxNumbering,
    pub ordered_numbering_id: usize,
    pub unordered_numbering_id: usize,
    pub current_ordered_instance: usize,
    pub current_unordered_instance: usize,
    pub numbered_levels: Vec<(usize, bool)>,
}

impl DocxConverter {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            list_level: 0,
            docx: Docx::new(),
            content_builder: ContentBuilder::new(),
            styles: DocxStyles::new(),
            numbering: DocxNumbering::new(),
            ordered_numbering_id: 1,
            unordered_numbering_id: 2,
            current_ordered_instance: 1,
            current_unordered_instance: 2,
            numbered_levels: Vec::new(),
        }
    }

    /// 初始化文档样式
    fn initialize_styles(&mut self) {
        self.docx = self.styles.initialize_styles(self.docx.clone());
    }

    /// 初始化文档编号
    fn initialize_numbering(&mut self) {
        self.docx = self.numbering.initialize_numbering(self.docx.clone());
    }

    /// 初始化文档 - 同时初始化样式和编号
    fn initialize_document(&mut self) {
        self.initialize_styles();
        self.initialize_numbering();
    }

    /// 刷新当前运行对象到段落中
    fn flush_run(&mut self) -> Result<()> {
        self.content_builder.flush_run()
    }

    /// 刷新当前段落到文档中
    fn flush_paragraph(&mut self) -> Result<()> {
        self.flush_run()?;

        // 取出当前段落
        if let Some(para) = self.content_builder.take_paragraph() {
            // 只有非空段落才添加到文档中
            if !para.children.is_empty() {
                self.docx = self.docx.clone().add_paragraph(para);
            }
        }

        self.content_builder.set_paragraph(Paragraph::new());

        Ok(())
    }

    /// 重置列表编号状态到指定层级
    fn reset_list_numbering(&mut self, to_level: usize) -> Result<()> {
        if !self.numbered_levels.is_empty() {
            self.numbered_levels.retain(|(level, _)| *level <= to_level);
        }

        Ok(())
    }

    /// 处理列表元素
    fn process_list(&mut self, root: &HtmlElement, list_state: ListState) -> Result<()> {
        let prev_state = self.list_state;
        let prev_level = self.list_level;

        self.list_state = Some(list_state);
        self.list_level = prev_level + 1;

        let is_ordered = matches!(list_state, ListState::Ordered);

        let list_type_changed = if !self.numbered_levels.is_empty() {
            self.numbered_levels
                .iter()
                .find(|(level, _)| *level == self.list_level)
                .map(|(_, prev_is_ordered)| *prev_is_ordered != is_ordered)
                .unwrap_or(false)
        } else {
            false
        };

        if list_type_changed {
            if is_ordered {
                self.ordered_numbering_id += 2;
            } else {
                self.unordered_numbering_id += 2;
            }
        }

        self.numbered_levels
            .retain(|(level, _)| *level != self.list_level);
        self.numbered_levels.push((self.list_level, is_ordered));

        if !self.content_builder.text_buffer.is_empty() {
            self.flush_paragraph()?;
        }

        self.convert_children(root)?;

        if !self.content_builder.text_buffer.is_empty() {
            self.flush_paragraph()?;
        }

        self.reset_list_numbering(prev_level)?;

        self.list_level = prev_level;
        self.list_state = prev_state;

        Ok(())
    }

    /// 获取或创建列表编号 ID
    fn get_or_create_numbering_id(&mut self, is_ordered: bool) -> usize {
        if is_ordered {
            self.ordered_numbering_id
        } else {
            self.unordered_numbering_id
        }
    }

    /// 主要转换方法 - 递归处理 HTML 元素
    pub fn convert(&mut self, root: &HtmlElement) -> Result<()> {
        self.initialize_document();

        match root.tag {
            tag::head => Ok(()),
            tag::html | tag::body | md_tag::doc => self.convert_children(root),
            tag::p | tag::span => self.convert_children(root),
            tag::dl | tag::dt | tag::dd => self.convert_children(root),
            tag::ol => self.process_list(root, ListState::Ordered),
            tag::ul => self.process_list(root, ListState::Unordered),
            tag::li => self.process_list_item(root),
            tag::div | tag::figure => {
                self.flush_run()?;
                self.convert_children(root)
            }
            tag::figcaption => self
                .create_styled_paragraph("Caption", |converter| converter.convert_children(root)),
            tag::pre => self.process_pre_block(root),
            md_tag::heading => self.convert_heading(root),
            md_tag::link => self.process_link(root),
            md_tag::parbreak => {
                self.flush_paragraph()?;
                Ok(())
            }
            md_tag::linebreak => {
                self.content_builder.add_line_break();
                Ok(())
            }
            tag::strong | md_tag::strong => {
                self.process_with_style("Strong", |converter| converter.convert_children(root))
            }
            tag::em | md_tag::emph => {
                self.process_with_style("Emphasis", |converter| converter.convert_children(root))
            }
            md_tag::highlight => {
                self.process_with_style("Highlight", |converter| converter.convert_children(root))
            }
            md_tag::strike => {
                self.process_with_style("Strike", |converter| converter.convert_children(root))
            }
            md_tag::raw => self.process_raw_code(root),
            md_tag::label | md_tag::reference | md_tag::outline | md_tag::outline_entry => {
                self.process_with_style("CodeInline", |converter| converter.convert_children(root))
            }
            md_tag::quote => self.create_styled_paragraph("Blockquote", |converter| {
                converter.convert_children(root)
            }),
            md_tag::table | md_tag::grid => self.process_table(root),
            md_tag::table_cell | md_tag::grid_cell => self.convert_children(root),
            md_tag::math_equation_inline | md_tag::math_equation_block => self.process_math(root),
            md_tag::image => self.process_image_element(root),
            _ => self.process_unknown_tag(root),
        }
    }

    /// 处理预格式化代码块
    fn process_pre_block(&mut self, root: &HtmlElement) -> Result<()> {
        let buffer = self.content_builder.get_buffer_clone();

        self.process_block_element(Some("CodeBlock"), |converter| {
            let lines = buffer.split('\n');
            let mut first_line = true;

            for line in lines {
                if !first_line {
                    // 添加换行
                    if let Some(run) = converter.content_builder.take_run() {
                        converter
                            .content_builder
                            .set_run(run.add_break(BreakType::TextWrapping));
                    }
                }
                converter.content_builder.add_text(line);
                converter.flush_run()?;
                first_line = false;
            }

            Ok(())
        })?;

        self.content_builder.clear_buffer();
        Ok(())
    }

    /// 处理列表项
    fn process_list_item(&mut self, root: &HtmlElement) -> Result<()> {
        self.flush_run()?;

        let mut paragraph = Paragraph::new();

        if let Some(list_state) = self.list_state {
            let is_ordered = matches!(list_state, ListState::Ordered);
            let numbering_id = self.get_or_create_numbering_id(is_ordered);
            let level = IndentLevel::new(self.list_level.saturating_sub(1));
            paragraph = paragraph.numbering(NumberingId::new(numbering_id), level);
        }

        self.content_builder.set_paragraph(paragraph);

        self.convert_children(root)?;

        if !self.content_builder.text_buffer.is_empty() {
            self.flush_paragraph()?;
        }

        Ok(())
    }

    /// 处理数学公式
    fn process_math(&mut self, root: &HtmlElement) -> Result<()> {
        // 判断是否是块级公式
        let is_block = root.tag == md_tag::math_equation_block;

        // 查找 Frame 子元素
        let maybe_frame = root.children.iter().find_map(|child| {
            if let HtmlNode::Frame(frame) = child {
                Some(frame)
            } else {
                None
            }
        });

        // 如果找不到帧，则添加占位符文本
        if maybe_frame.is_none() {
            if is_block {
                return self.process_block_element(Some("MathBlock"), |converter| {
                    converter.content_builder.add_text("[Math Expression]");
                    Ok(())
                });
            } else {
                return self.process_inline_element(None, |converter| {
                    converter.content_builder.add_text("[Math Expression]");
                    Ok(())
                });
            }
        }

        let frame = maybe_frame.unwrap();

        // 渲染公式为 PNG
        let png_data = match self.render_frame_to_png(frame) {
            Ok(data) => data,
            Err(e) => return Err(e),
        };

        let (width, height) = self.calculate_image_dimensions(&png_data, Some(96.0 / 300.0 / 2.0));
        let pic = Pic::new(&png_data).size(width, height);

        // 根据是块级还是内联级处理
        if is_block {
            self.process_block_element(Some("MathBlock"), |converter| {
                if let Some(para) = converter.content_builder.take_paragraph() {
                    converter
                        .content_builder
                        .set_paragraph(para.add_run(Run::new().add_image(pic)));
                }
                Ok(())
            })
        } else {
            self.process_inline_element(None, |converter| {
                if let Some(run) = converter.content_builder.take_run() {
                    converter.content_builder.set_run(run.add_image(pic));
                }
                Ok(())
            })
        }
    }

    /// 处理图片元素
    fn process_image_element(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&root.attrs)?;
        let src = unix_slash(Path::new(attrs.src.as_str()));

        if let Ok(img_data) = std::fs::read(&src) {
            self.process_image(&img_data, &attrs.alt)
        } else {
            // 对于找不到的图片，添加占位符文本
            self.process_inline_element(None, |converter| {
                converter
                    .content_builder
                    .add_text(&format!("[Image: {}]", attrs.alt));
                Ok(())
            })
        }
    }

    /// 处理图片数据
    fn process_image(&mut self, img_data: &[u8], alt_text: &str) -> Result<()> {
        // 图片总是创建为块级元素
        let (width, height) = self.calculate_image_dimensions(img_data, None);
        let pic = Pic::new(img_data).size(width, height);

        // 图片处理逻辑
        self.process_block_element(None, |converter| {
            let pic_para = converter
                .content_builder
                .take_paragraph()
                .unwrap_or_else(|| Paragraph::new())
                .add_run(Run::new().add_image(pic.clone()));
            converter.content_builder.set_paragraph(pic_para);
            Ok(())
        })?;

        // 如果有 alt 文本，添加图片说明
        if !alt_text.is_empty() {
            self.process_block_element(Some("Caption"), |converter| {
                converter.content_builder.add_text(alt_text);
                Ok(())
            })?;
        }

        Ok(())
    }

    /// 处理未知标签
    fn process_unknown_tag(&mut self, root: &HtmlElement) -> Result<()> {
        self.content_builder
            .add_text(&format!("[Unknown tag: {:?}]", root.tag));
        self.flush_run()?;
        self.convert_children(root)?;
        Ok(())
    }

    /// 递归处理子元素
    pub fn convert_children(&mut self, root: &HtmlElement) -> Result<()> {
        for child in &root.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => self.process_frame(frame, false)?,
                HtmlNode::Text(text, _) => {
                    self.content_builder.add_text(text);
                }
                HtmlNode::Element(element) => {
                    self.convert(element)?;
                }
            }
        }
        Ok(())
    }

    /// 处理标题元素
    fn convert_heading(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = HeadingAttr::parse(&root.attrs)?;

        if attrs.level >= 7 {
            return Err(format!("heading level {} is too high", attrs.level).into());
        }

        // 根据标题级别确定样式名称
        let style_name = match attrs.level {
            1 => "Heading1",
            2 => "Heading2",
            3 => "Heading3",
            4 => "Heading4",
            5 => "Heading5",
            _ => "Heading6",
        };

        self.process_block_element(Some(style_name), |converter| {
            converter.convert_children(root)
        })
    }

    /// 将帧渲染为 PNG 图像
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

    /// 计算图片尺寸
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
            // 默认尺寸
            (4000000, 3000000)
        }
    }

    /// 处理帧元素
    fn process_frame(&mut self, frame: &Frame, block: bool) -> Result<()> {
        // 注意：这个方法不用于处理数学公式帧，那些在 process_math 中处理
        if block {
            return Err("Block frames should be handled in process_math".into());
        }

        self.flush_run()?;

        let png_data = self.render_frame_to_png(frame)?;
        let (width, height) = self.calculate_image_dimensions(&png_data, Some(96.0 / 300.0 / 2.0));
        let pic = Pic::new(&png_data).size(width, height);

        if let Some(run) = self.content_builder.take_run() {
            self.content_builder.set_run(run.add_image(pic));
        }

        Ok(())
    }

    /// 将内容导出为 DOCX 格式
    pub fn to_docx(&mut self) -> Result<Vec<u8>> {
        self.flush_paragraph()?;

        let docx = self.docx.clone().build();
        let mut buffer = Vec::new();
        docx.pack(&mut std::io::Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;
        Ok(buffer)
    }

    /// 处理内联元素
    fn process_inline_element<F>(&mut self, style_name: Option<&str>, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.flush_run()?;

        let prev_run = self.content_builder.take_run();
        let mut new_run = Run::new();

        // 如果提供了样式名，则应用样式
        if let Some(style) = style_name {
            new_run = new_run.style(style);
        }

        self.content_builder.set_run(new_run);

        // 处理内容
        process_fn(self)?;

        self.flush_run()?;

        // 恢复之前的运行
        self.content_builder
            .set_run(prev_run.unwrap_or_else(|| Run::new()));

        Ok(())
    }

    /// 处理块级元素
    fn process_block_element<F>(&mut self, style_name: Option<&str>, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.flush_run()?;

        // 保存当前段落和运行
        let prev_para = self.content_builder.take_paragraph();
        let prev_run = self.content_builder.take_run();

        // 创建新的段落，如果提供了样式则应用样式
        let mut new_para = Paragraph::new();
        if let Some(style) = style_name {
            new_para = new_para.style(style);
        }

        self.content_builder.set_paragraph(new_para);
        self.content_builder.set_run(Run::new());

        // 处理内容
        process_fn(self)?;

        // 刷新并添加到文档
        self.flush_paragraph()?;

        // 恢复之前的状态
        self.content_builder
            .set_paragraph(prev_para.unwrap_or_else(|| Paragraph::new()));
        self.content_builder
            .set_run(prev_run.unwrap_or_else(|| Run::new()));

        Ok(())
    }

    /// 使用指定样式处理内容
    fn process_with_style<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.process_inline_element(Some(style_name), process_fn)
    }

    /// 创建带样式的段落
    fn create_styled_paragraph<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.process_block_element(Some(style_name), process_fn)
    }

    /// 处理链接元素
    fn process_link(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&root.attrs)?;

        // 使用内联样式处理链接文本
        self.process_inline_element(None, |converter| {
            // 处理链接内容
            converter.convert_children(root)?;

            // 创建超链接
            if let Some(para) = converter.content_builder.take_paragraph() {
                // 获取当前运行内容
                converter.flush_run()?;
                let run = converter
                    .content_builder
                    .take_run()
                    .unwrap_or_else(|| Run::new());

                // 创建超链接
                let hyperlink = Hyperlink::new(&attrs.dest, HyperlinkType::External).add_run(run);

                converter
                    .content_builder
                    .set_paragraph(para.add_hyperlink(hyperlink));
            }

            Ok(())
        })
    }

    /// 处理原始代码
    fn process_raw_code(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = RawAttr::parse(&root.attrs)?;

        if attrs.block {
            self.process_code_block(&attrs)
        } else {
            self.process_inline_code(&attrs)
        }
    }

    /// 处理代码块
    fn process_code_block(&mut self, attrs: &RawAttr) -> Result<()> {
        self.process_block_element(Some("CodeBlock"), |converter| {
            // 如果有语言标记，先添加语言标签
            if !attrs.lang.is_empty() {
                converter.content_builder.set_run(
                    Run::new()
                        .add_text(format!("Language: {}", attrs.lang))
                        .italic(),
                );
                converter.flush_run()?;

                // 添加换行
                if let Some(run) = converter.content_builder.take_run() {
                    converter
                        .content_builder
                        .set_run(run.add_break(BreakType::TextWrapping));
                }
            }

            // 处理代码行
            let lines = attrs.text.split('\n');
            let mut first_line = true;

            for line in lines {
                if !first_line {
                    // 添加换行
                    if let Some(run) = converter.content_builder.take_run() {
                        converter
                            .content_builder
                            .set_run(run.add_break(BreakType::TextWrapping));
                    }
                }
                converter.content_builder.add_text(line);
                converter.flush_run()?;
                first_line = false;
            }

            Ok(())
        })
    }

    /// 处理内联代码
    fn process_inline_code(&mut self, attrs: &RawAttr) -> Result<()> {
        self.process_inline_element(Some("CodeInline"), |converter| {
            converter.content_builder.add_text(&attrs.text);
            Ok(())
        })
    }

    /// 处理表格
    fn process_table(&mut self, root: &HtmlElement) -> Result<()> {
        self.process_block_element(Some("Table"), |converter| {
            // 处理表格行
            let mut rows = Vec::new();
            for child in &root.children {
                if let HtmlNode::Element(element) = child {
                    rows.push(converter.process_table_row(element)?);
                }
            }

            // 创建表格并添加行
            let mut table = Table::new(vec![]).style("Table");
            for row in rows {
                table = table.add_row(row);
            }

            // 将表格添加到文档
            converter.docx = converter.docx.clone().add_table(table);

            Ok(())
        })
    }

    /// 处理表格行
    fn process_table_row(&mut self, row_element: &HtmlElement) -> Result<TableRow> {
        let mut cells = Vec::new();
        for child in &row_element.children {
            if let HtmlNode::Element(cell_element) = child {
                let cell = self.process_table_cell(cell_element)?;
                cells.push(cell);
            }
        }
        Ok(TableRow::new(cells).cant_split())
    }

    /// 处理表格单元格
    fn process_table_cell(&mut self, cell_element: &HtmlElement) -> Result<TableCell> {
        let mut cell_paragraph = Paragraph::default();

        self.process_block_element(None, |converter| {
            // 处理单元格内的内容
            converter.convert_children(cell_element)?;

            // 保存生成的段落
            if let Some(para) = converter.content_builder.take_paragraph() {
                cell_paragraph = para;
            }

            Ok(())
        })?;

        // 创建并返回表格单元格
        Ok(TableCell::new().add_paragraph(cell_paragraph))
    }

    /// 开始文档处理
    pub fn begin_document(&mut self) -> Result<()> {
        self.initialize_document();
        Ok(())
    }

    /// 完成文档处理
    pub fn finish_document(&mut self) -> Result<()> {
        self.flush_paragraph()
    }
}
