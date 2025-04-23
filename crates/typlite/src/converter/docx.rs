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
    needs_new_paragraph: bool,
}

impl ContentBuilder {
    fn new() -> Self {
        Self {
            current_paragraph: Some(Paragraph::new()),
            current_run: Some(Run::new()),
            text_buffer: String::new(),
            needs_new_paragraph: false,
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
        if self.needs_new_paragraph && !text.trim().is_empty() {
            self.needs_new_paragraph = false;
        }
        self.text_buffer.push_str(text);
    }

    fn add_line_break(&mut self) {
        if !self.text_buffer.is_empty() && !self.text_buffer.ends_with('\n') {
            self.text_buffer.push('\n');
        }
    }

    fn take_paragraph(&mut self) -> Option<Paragraph> {
        self.flush_run().ok()?;
        self.current_paragraph.take()
    }

    fn set_paragraph(&mut self, paragraph: Paragraph) {
        self.current_paragraph = Some(paragraph);
        self.needs_new_paragraph = false;
    }

    fn mark_needs_new_paragraph(&mut self) {
        // 只有在文本缓冲区非空时才标记需要新段落
        if !self.text_buffer.trim().is_empty() {
            self.needs_new_paragraph = true;
        }
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

    fn initialize_styles(&mut self) {
        self.docx = self.styles.initialize_styles(self.docx.clone());
    }

    fn initialize_numbering(&mut self) {
        self.docx = self.numbering.initialize_numbering(self.docx.clone());
    }

    fn flush_run(&mut self) -> Result<()> {
        self.content_builder.flush_run()
    }

    fn flush_paragraph(&mut self) -> Result<()> {
        self.flush_run()?;

        // 取出当前段落
        if let Some(para) = self.content_builder.take_paragraph() {
            // 只有非空段落才添加到文档中
            if !para.children.is_empty() {
                self.docx = self.docx.clone().add_paragraph(para);
            }
        }

        // 创建新的段落，但不标记需要新段落
        // 这样避免插入不必要的空行
        self.content_builder.set_paragraph(Paragraph::new());

        Ok(())
    }

    fn reset_list_numbering(&mut self, to_level: usize) -> Result<()> {
        let is_empty = self.numbered_levels.is_empty();

        if !is_empty {
            self.numbered_levels.retain(|(level, _)| *level <= to_level);
        }

        Ok(())
    }

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

    fn get_or_create_numbering_id(&mut self, is_ordered: bool) -> usize {
        if is_ordered {
            self.ordered_numbering_id
        } else {
            self.unordered_numbering_id
        }
    }

    pub fn convert(&mut self, root: &HtmlElement) -> Result<()> {
        self.initialize_numbering();
        self.initialize_styles();

        match root.tag {
            tag::head => Ok(()),
            tag::html | tag::body | md_tag::doc => {
                self.convert_children(root)?;
                Ok(())
            }
            tag::p | tag::span => {
                self.convert_children(root)?;
                Ok(())
            }
            tag::dl | tag::dt | tag::dd => {
                self.convert_children(root)?;
                Ok(())
            }
            tag::ol => self.process_list(root, ListState::Ordered),
            tag::ul => self.process_list(root, ListState::Unordered),
            tag::li => self.process_list_item(root),
            tag::figure => {
                self.flush_run()?;
                self.convert_children(root)?;
                self.content_builder.mark_needs_new_paragraph();
                Ok(())
            }
            tag::figcaption => self
                .create_styled_paragraph("Caption", |converter| converter.convert_children(root)),
            tag::div => {
                self.convert_children(root)?;
                self.content_builder.mark_needs_new_paragraph();
                Ok(())
            }
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
            md_tag::table_cell | md_tag::grid_cell => {
                self.convert_children(root)?;
                Ok(())
            }
            md_tag::math_equation_inline | md_tag::math_equation_block => self.process_math(root),
            md_tag::image => self.process_image_element(root),
            _ => self.process_unknown_tag(root),
        }
    }

    fn process_pre_block(&mut self, _root: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;
        let mut code_para = Paragraph::new().style("CodeBlock");
        let buffer = self.content_builder.get_buffer_clone();
        let lines = buffer.split('\n');
        let mut first_line = true;
        for line in lines {
            if !first_line {
                code_para = code_para.add_run(Run::new().add_break(BreakType::TextWrapping));
            }
            code_para = code_para.add_run(Run::new().add_text(line));
            first_line = false;
        }
        self.docx = self.docx.clone().add_paragraph(code_para);
        self.content_builder.clear_buffer();
        self.content_builder.set_run(Run::new());
        self.content_builder.set_paragraph(Paragraph::new());
        Ok(())
    }

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

    fn process_math(&mut self, root: &HtmlElement) -> Result<()> {
        // 先保存当前的状态，以便在处理完公式后恢复
        let prev_paragraph = self.content_builder.take_paragraph();
        let prev_run = self.content_builder.take_run();

        if let Some(frame) = root.children.iter().find_map(|child| {
            if let HtmlNode::Frame(frame) = child {
                Some(frame)
            } else {
                None
            }
        }) {
            let is_block = root.tag == md_tag::math_equation_block;

            // 处理数学公式帧
            let png_data = self.render_frame_to_png(frame)?;
            let (width, height) =
                self.calculate_image_dimensions(&png_data, Some(96.0 / 300.0 / 2.0));
            let pic = Pic::new(&png_data).size(width, height);

            if is_block {
                // 对于块级公式，创建一个居中的新段落
                let math_para = Paragraph::new()
                    .style("MathBlock")
                    .add_run(Run::new().add_image(pic));

                // 先将之前的内容刷新到文档
                if prev_paragraph.is_some() {
                    self.content_builder.set_paragraph(prev_paragraph.unwrap());
                    self.flush_paragraph()?;
                }

                // 添加数学公式段落
                self.docx = self.docx.clone().add_paragraph(math_para);

                // 创建新的空段落继续后面的内容
                self.content_builder.set_paragraph(Paragraph::new());
                self.content_builder
                    .set_run(prev_run.unwrap_or_else(|| Run::new()));
            } else {
                // 行内公式只需添加到当前运行中
                self.content_builder
                    .set_paragraph(prev_paragraph.unwrap_or_else(|| Paragraph::new()));
                let run = prev_run.unwrap_or_else(|| Run::new()).add_image(pic);
                self.content_builder.set_run(run);
            }
        } else {
            // 如果没有找到帧，恢复之前的状态并添加一个占位符文本
            self.content_builder
                .set_paragraph(prev_paragraph.unwrap_or_else(|| Paragraph::new()));
            self.content_builder
                .set_run(prev_run.unwrap_or_else(|| Run::new()));
            self.content_builder.add_text("[Math Expression]");
            self.flush_run()?;
        }

        Ok(())
    }

    fn process_image_element(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&root.attrs)?;
        let src = unix_slash(Path::new(attrs.src.as_str()));

        if let Ok(img_data) = std::fs::read(&src) {
            self.flush_run()?;
            self.process_image(&img_data, &attrs.alt)?;
        } else {
            self.content_builder
                .add_text(&format!("[Image: {}]", attrs.alt));
            self.flush_run()?;
        }

        Ok(())
    }

    fn process_unknown_tag(&mut self, root: &HtmlElement) -> Result<()> {
        self.content_builder
            .add_text(&format!("[Unknown tag: {:?}]", root.tag));
        self.flush_run()?;
        self.convert_children(root)?;
        Ok(())
    }

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

    fn convert_heading(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = HeadingAttr::parse(&root.attrs)?;

        if attrs.level >= 7 {
            return Err(format!("heading level {} is too high", attrs.level).into());
        }

        // 标题内容通常放在一个单独的段落中，所以先刷新当前段落
        if !self.content_builder.text_buffer.is_empty() {
            self.flush_paragraph()?;
        }

        // 创建一个带有标题样式的段落
        let style_name = match attrs.level {
            1 => "Heading1",
            2 => "Heading2",
            3 => "Heading3",
            4 => "Heading4",
            5 => "Heading5",
            _ => "Heading6",
        };

        // 使用 create_styled_paragraph 处理标题内容
        self.create_styled_paragraph(style_name, |converter| {
            converter.convert_children(root)
        })
        // 不再标记需要段落，而是让实际内容来决定何时创建新段落
    }

    fn render_frame_to_png(&self, frame: &Frame) -> Result<Vec<u8>> {
        let svg = typst_svg::svg_frame(frame);

        let png_data = {
            let dpi = 300.0;
            let scale_factor = dpi / 96.0;

            let opt = Options {
                dpi: dpi,
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
                .map_err(|e| format!("PNG encode error: {:?}", e))?
        };

        Ok(png_data)
    }

    fn calculate_image_dimensions(&self, img_data: &[u8], scale_factor: Option<f32>) -> (u32, u32) {
        let actual_scale = scale_factor.unwrap_or(1.0);
        let img_size = get_image_size(img_data);
        match img_size {
            Some((w, h)) => {
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
            }
            None => (4000000, 3000000),
        }
    }

    fn process_frame(&mut self, frame: &Frame, block: bool) -> Result<()> {
        // 注意：现在这个方法已经不用于处理数学公式帧
        // 因为数学公式帧的处理已经在 process_math 中完成
        if block {
            // 警告：这个分支不应该直接用于处理块级数学公式
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

    pub fn to_docx(&mut self) -> Result<Vec<u8>> {
        self.flush_paragraph()?;

        let docx = self.docx.clone().build();
        let mut buffer = Vec::new();
        docx.pack(&mut std::io::Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;
        Ok(buffer)
    }

    fn process_image(&mut self, img_data: &[u8], alt_text: &str) -> Result<()> {
        let (width, height) = self.calculate_image_dimensions(img_data, None);

        let pic = Pic::new(img_data).size(width, height);
        let pic_para = Paragraph::new().add_run(Run::new().add_image(pic));
        self.docx = self.docx.clone().add_paragraph(pic_para);

        if !alt_text.is_empty() {
            self.add_caption(alt_text);
        }

        Ok(())
    }

    fn add_caption(&mut self, caption_text: &str) {
        let caption = Paragraph::new()
            .add_run(Run::new().add_text(caption_text).italic().size(18))
            .style("Caption");

        self.docx = self.docx.clone().add_paragraph(caption);
    }

    fn process_with_style<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.flush_run()?;

        let prev_run = self.content_builder.take_run();
        self.content_builder.set_run(Run::new().style(style_name));

        process_fn(self)?;

        self.flush_run()?;

        self.content_builder
            .set_run(prev_run.unwrap_or_else(|| Run::new()));

        Ok(())
    }

    fn create_styled_paragraph<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let styled_para = Paragraph::new().style(style_name);
        let prev_para = self.content_builder.take_paragraph();
        self.content_builder.set_paragraph(styled_para);

        let result = process_fn(self);
        self.flush_paragraph()?;

        self.content_builder
            .set_paragraph(prev_para.unwrap_or_else(|| Paragraph::new()));
        result
    }

    fn process_link(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&root.attrs)?;

        let prev_run = self.content_builder.take_run();
        self.content_builder
            .set_run(Run::new().color("0000FF").underline("single"));

        self.convert_children(root)?;
        self.flush_run()?;

        if let Some(para) = self.content_builder.take_paragraph() {
            let hyperlink = Hyperlink::new(&attrs.dest, HyperlinkType::External).add_run(
                Run::new()
                    .add_text(self.content_builder.get_buffer_clone())
                    .color("0000FF")
                    .underline("single"),
            );

            self.content_builder
                .set_paragraph(para.add_hyperlink(hyperlink));
        }

        self.content_builder.clear_buffer();
        self.content_builder
            .set_run(prev_run.unwrap_or_else(|| Run::new()));

        Ok(())
    }

    fn process_raw_code(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = RawAttr::parse(&root.attrs)?;

        if attrs.block {
            self.process_code_block(&attrs)
        } else {
            self.process_inline_code(&attrs)
        }
    }

    fn process_code_block(&mut self, attrs: &RawAttr) -> Result<()> {
        self.flush_paragraph()?;

        let mut code_para = Paragraph::new().style("CodeBlock");

        if !attrs.lang.is_empty() {
            code_para = code_para.add_run(
                Run::new()
                    .add_text(format!("Language: {}\n", attrs.lang))
                    .italic(),
            );
        }

        let lines = attrs.text.split('\n');
        let mut first_line = true;

        for line in lines {
            if !first_line {
                code_para = code_para.add_run(Run::new().add_break(BreakType::TextWrapping));
            }
            code_para = code_para.add_run(Run::new().add_text(line));
            first_line = false;
        }

        self.docx = self.docx.clone().add_paragraph(code_para);
        self.content_builder.needs_new_paragraph = false;
        Ok(())
    }

    fn process_inline_code(&mut self, attrs: &RawAttr) -> Result<()> {
        self.flush_run()?;
        let code_run = Run::new().add_text(&attrs.text).style("CodeInline");
        if let Some(para) = self.content_builder.take_paragraph() {
            self.content_builder.set_paragraph(para.add_run(code_run));
        }
        self.content_builder.set_run(Run::new());
        Ok(())
    }

    fn process_table(&mut self, root: &HtmlElement) -> Result<()> {
        let mut table = Table::new(vec![]).style("Table");
        let current_docx = self.docx.clone();

        let mut rows = Vec::new();
        for child in &root.children {
            if let HtmlNode::Element(element) = child {
                rows.push(self.process_table_row(element)?);
            }
        }

        for row in rows {
            table = table.add_row(row);
        }

        self.docx = current_docx.add_table(table);
        self.content_builder.mark_needs_new_paragraph();

        Ok(())
    }

    fn process_table_row(&mut self, row_element: &HtmlElement) -> Result<TableRow> {
        let mut cells = Vec::new();
        for child in &row_element.children {
            if let HtmlNode::Element(cell_element) = child {
                let cell = self.process_table_cell(cell_element)?;
                cells.push(cell);
            }
        }
        let row = TableRow::new(cells).cant_split();
        Ok(row)
    }

    fn process_table_cell(&mut self, cell_element: &HtmlElement) -> Result<TableCell> {
        let prev_paragraph = self.content_builder.take_paragraph();
        let prev_run = self.content_builder.take_run();

        self.content_builder.set_paragraph(Paragraph::new());
        self.content_builder.set_run(Run::new());
        self.content_builder.clear_buffer();

        self.convert_children(cell_element)?;
        self.flush_run()?;

        let cell_paragraph = self.content_builder.take_paragraph().unwrap_or_default();

        self.content_builder
            .set_paragraph(prev_paragraph.unwrap_or_else(|| Paragraph::new()));
        self.content_builder
            .set_run(prev_run.unwrap_or_else(|| Run::new()));

        Ok(TableCell::new().add_paragraph(cell_paragraph))
    }

    pub fn begin_document(&mut self) -> Result<()> {
        self.initialize_styles();
        self.initialize_numbering();
        Ok(())
    }

    pub fn finish_document(&mut self) -> Result<()> {
        self.flush_paragraph()?;
        Ok(())
    }
}
