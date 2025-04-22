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

fn extract_svg_dimensions(svg: &str) -> Option<(f32, f32)> {
    let width_re = regex::Regex::new(r#"width="([0-9.]+)([a-z]+)?"#).ok()?;
    let height_re = regex::Regex::new(r#"height="([0-9.]+)([a-z]+)?"#).ok()?;

    let width_cap = width_re.captures(svg)?;
    let width = width_cap.get(1)?.as_str().parse::<f32>().ok()?;

    let height_cap = height_re.captures(svg)?;
    let height = height_cap.get(1)?.as_str().parse::<f32>().ok()?;

    Some((width, height))
}

/// DOCX converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    docx: Docx,
    current_paragraph: Option<Paragraph>,
    current_run: Option<Run>,
    text_buffer: String,
    numbering_initialized: bool,
    styles_initialized: bool,
}

impl DocxConverter {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            list_level: 0,
            docx: Docx::new(),
            current_paragraph: Some(Paragraph::new()),
            current_run: Some(Run::new()),
            text_buffer: String::new(),
            numbering_initialized: false,
            styles_initialized: false,
        }
    }

    fn create_heading_style(name: &str, display_name: &str, size: usize) -> Style {
        Style::new(name, StyleType::Paragraph)
            .name(display_name)
            .size(size)
            .bold()
    }

    fn initialize_styles(&mut self) {
        if self.styles_initialized {
            return;
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

        self.docx = self
            .docx
            .clone()
            .add_style(heading1)
            .add_style(heading2)
            .add_style(heading3)
            .add_style(heading4)
            .add_style(heading5)
            .add_style(heading6)
            .add_style(code_block)
            .add_style(code_inline)
            .add_style(emphasis)
            .add_style(strong)
            .add_style(highlight)
            .add_style(blockquote)
            .add_style(caption)
            .add_style(table);

        self.styles_initialized = true;
    }

    fn create_list_level(_id: u32, format: &str, text: &str, _is_bullet: bool) -> Level {
        let level = Level::new(
            0,
            Start::new(1),
            NumberFormat::new(format),
            LevelText::new(text),
            LevelJc::new("left"),
        );

        level.indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None)
    }

    fn initialize_numbering(&mut self) {
        if self.numbering_initialized {
            return;
        }

        let ordered_level = Self::create_list_level(0, "decimal", "%1.", false);
        let unordered_level = Self::create_list_level(0, "bullet", "•", true);

        let ordered_abstract_numbering = AbstractNumbering::new(1).add_level(ordered_level);
        let unordered_abstract_numbering = AbstractNumbering::new(2).add_level(unordered_level);

        self.docx = self
            .docx
            .clone()
            .add_abstract_numbering(ordered_abstract_numbering)
            .add_abstract_numbering(unordered_abstract_numbering);

        let ordered_numbering = Numbering::new(1, 1); // numbering_id, abstract_numbering_id
        let unordered_numbering = Numbering::new(2, 2);

        self.docx = self
            .docx
            .clone()
            .add_numbering(ordered_numbering)
            .add_numbering(unordered_numbering);

        self.numbering_initialized = true;
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
            // todo: handle description list
            tag::dl | tag::dt | tag::dd => {
                self.flush_paragraph()?;
                self.convert_children(root)?;
                Ok(())
            }
            tag::ol => {
                let state = self.list_state;
                self.list_state = Some(ListState::Ordered);
                self.list_level += 1;
                self.flush_paragraph()?;

                self.convert_children(root)?;

                self.list_level -= 1;
                self.list_state = state;
                Ok(())
            }
            tag::ul => {
                let state = self.list_state;
                self.list_state = Some(ListState::Unordered);
                self.list_level += 1;
                self.flush_paragraph()?;

                self.convert_children(root)?;

                self.list_level -= 1;
                self.list_state = state;
                Ok(())
            }
            tag::li => {
                // self.flush_paragraph()?;
                self.current_paragraph = Some(Paragraph::new());

                if let Some(list_state) = self.list_state {
                    let level = IndentLevel::new(self.list_level.saturating_sub(1));
                    // println!("List level: {}", self.list_level.saturating_sub(1));
                    match list_state {
                        ListState::Ordered => {
                            self.current_paragraph = Some(
                                self.current_paragraph
                                    .take()
                                    .unwrap()
                                    .numbering(NumberingId::new(1), level),
                            );
                        }
                        ListState::Unordered => {
                            self.current_paragraph = Some(
                                self.current_paragraph
                                    .take()
                                    .unwrap()
                                    .numbering(NumberingId::new(2), level),
                            );
                        }
                    }
                }

                self.convert_children(root)?;
                self.flush_paragraph()?;

                Ok(())
            }
            tag::figure => {
                self.flush_run()?;
                self.convert_children(root)?;
                Ok(())
            }
            tag::figcaption => self
                .create_styled_paragraph("Caption", |converter| converter.convert_children(root)),
            tag::div => {
                self.flush_paragraph()?;
                self.convert_children(root)?;
                Ok(())
            }
            tag::pre => {
                // should be treated as a code block
                self.flush_paragraph()?;
                let mut code_para = Paragraph::new().style("CodeBlock");
                let lines = self.text_buffer.split('\n');
                let mut first_line = true;
                for line in lines {
                    if !first_line {
                        code_para =
                            code_para.add_run(Run::new().add_break(BreakType::TextWrapping));
                    }
                    code_para = code_para.add_run(Run::new().add_text(line));
                    first_line = false;
                }
                self.docx = self.docx.clone().add_paragraph(code_para);
                self.text_buffer.clear();
                self.current_run = Some(Run::new());
                self.current_paragraph = Some(Paragraph::new());
                Ok(())
            }
            md_tag::heading => self.convert_heading(root),
            md_tag::link => self.process_link(root),
            md_tag::parbreak => {
                self.flush_paragraph()?;
                Ok(())
            }
            md_tag::linebreak => {
                self.text_buffer.push('\n');
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
            md_tag::math_equation_inline | md_tag::math_equation_block => {
                if let Some(frame) = root.children.iter().find_map(|child| {
                    if let HtmlNode::Frame(frame) = child {
                        Some(frame)
                    } else {
                        None
                    }
                }) {
                    self.process_frame(frame, root.tag == md_tag::math_equation_block)?;
                } else {
                    self.text_buffer.push_str("[Math Expression]");
                    self.flush_run()?;
                }
                Ok(())
            }
            md_tag::image => {
                let attrs = ImageAttr::parse(&root.attrs)?;
                let src = unix_slash(Path::new(attrs.src.as_str()));

                if let Ok(img_data) = std::fs::read(&src) {
                    self.flush_run()?;
                    self.process_image(&img_data, &attrs.alt)?;
                } else {
                    self.text_buffer
                        .push_str(&format!("[Image: {}]", attrs.alt));
                    self.flush_run()?;
                }

                Ok(())
            }
            _ => {
                self.text_buffer
                    .push_str(&format!("[Unknown tag: {:?}]", root.tag));
                self.flush_run()?;
                self.convert_children(root)?;
                Ok(())
            }
        }
    }

    pub fn convert_children(&mut self, root: &HtmlElement) -> Result<()> {
        for child in &root.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => {
                    // println!("Processing frame in root: {:#?}", root);
                    self.process_frame(frame, true)?
                }
                HtmlNode::Text(text, _) => {
                    self.text_buffer.push_str(text);
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

        self.create_styled_paragraph(
            match attrs.level {
                1 => "Heading1",
                2 => "Heading2",
                3 => "Heading3",
                4 => "Heading4",
                5 => "Heading5",
                _ => "Heading6",
            },
            |converter| converter.convert_children(root),
        )
    }

    fn process_frame(&mut self, frame: &Frame, flush_par: bool) -> Result<()> {
        let svg = typst_svg::svg_frame(frame);

        if flush_par {
            self.flush_paragraph()?;
        } else {
            self.flush_run()?;
        }

        // Convert SVG to PNG using resvg
        let png_data = {
            let opt = Options::default();
            let rtree = match Tree::from_str(&svg, &opt) {
                Ok(tree) => tree,
                Err(e) => {
                    eprintln!("SVG parse error: {:?}, {:?}", e, typst_svg::svg_frame(frame));
                    return Ok(()); // 不中断，直接返回
                }
            };
            let size = rtree.size().to_int_size();
            let mut pixmap =
                Pixmap::new(size.width(), size.height()).ok_or("Failed to create pixmap")?;
            resvg::render(
                &rtree,
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
            );
            pixmap
                .encode_png()
                .map_err(|e| format!("PNG encode error: {:?}", e))?
        };
        let (width, height) = self.calculate_image_dimensions(&png_data);
        let pic = Pic::new(&png_data).size(width, height);

        if flush_par {
            // Create a new paragraph with the image
            let pic_para = Paragraph::new().add_run(Run::new().add_image(pic));
            self.docx = self.docx.clone().add_paragraph(pic_para);
        } else {
            // Add the image to the current run
            if let Some(ref mut run) = self.current_run {
                *run = run.clone().add_image(pic);
            }
        }

        Ok(())
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

    fn flush_paragraph(&mut self) -> Result<()> {
        self.flush_run()?;

        if let Some(para) = self.current_paragraph.take() {
            self.docx = self.docx.clone().add_paragraph(para);
        }

        self.current_paragraph = Some(Paragraph::new());

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
        let (width, height) = self.calculate_image_dimensions(img_data);

        let pic = Pic::new(img_data).size(width, height);
        let pic_para = Paragraph::new().add_run(Run::new().add_image(pic));
        self.docx = self.docx.clone().add_paragraph(pic_para);

        if !alt_text.is_empty() {
            self.add_caption(alt_text);
        }

        Ok(())
    }

    fn calculate_image_dimensions(&self, img_data: &[u8]) -> (u32, u32) {
        let img_size = get_image_size(img_data);
        match img_size {
            Some((w, h)) => {
                let max_width = 5486400;
                if w > max_width {
                    let ratio = h as f32 / w as f32;
                    let scaled_height = (max_width as f32 * ratio) as u32;
                    (max_width, scaled_height)
                } else {
                    (w * 9525, h * 9525)
                }
            }
            None => (4000000, 3000000),
        }
    }

    fn add_caption(&mut self, caption_text: &str) {
        let caption = Paragraph::new()
            .add_run(Run::new().add_text(caption_text).italic().size(18))
            .style("Caption");

        self.docx = self.docx.clone().add_paragraph(caption);
    }

    fn calculate_svg_dimensions(&self, svg: &str) -> (u32, u32) {
        if let Some((w, h)) = extract_svg_dimensions(svg) {
            // 1 点 = 12700 EMU
            let emu_w = (w * 12700.0) as u32;
            let emu_h = (h * 12700.0) as u32;

            let max_width = 5486400;
            if emu_w > max_width {
                let ratio = emu_h as f32 / emu_w as f32;
                let scaled_height = (max_width as f32 * ratio) as u32;
                (max_width, scaled_height)
            } else {
                (emu_w, emu_h)
            }
        } else {
            (4000000, 3000000)
        }
    }

    fn process_with_style<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.flush_run()?;
        process_fn(self)?;
        if !self.text_buffer.is_empty() {
            let code_run = Run::new()
                .add_text(self.text_buffer.clone())
                .style(style_name);
            if let Some(ref mut para) = self.current_paragraph {
                *para = para.clone().add_run(code_run);
            }
            self.text_buffer.clear();
        }
        self.current_run = Some(Run::new());
        Ok(())
    }

    fn create_styled_paragraph<F>(&mut self, style_name: &str, process_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.flush_paragraph()?;

        let styled_para = Paragraph::new().style(style_name);
        let prev_para = self.current_paragraph.take();
        self.current_paragraph = Some(styled_para);

        let result = process_fn(self);
        self.flush_paragraph()?;

        self.current_paragraph = prev_para;
        result
    }

    fn process_link(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&root.attrs)?;

        let prev_run = self.current_run.take();
        self.current_run = Some(Run::new().color("0000FF").underline("single"));

        self.convert_children(root)?;
        self.flush_run()?;

        if let Some(ref mut para) = self.current_paragraph {
            let hyperlink = Hyperlink::new(&attrs.dest, HyperlinkType::External).add_run(
                Run::new()
                    .add_text(self.text_buffer.clone())
                    .color("0000FF")
                    .underline("single"),
            );

            *para = para.clone().add_hyperlink(hyperlink);
        }

        self.text_buffer.clear();
        self.current_run = prev_run;

        Ok(())
    }

    fn process_raw_code(&mut self, root: &HtmlElement) -> Result<()> {
        let attrs = RawAttr::parse(&root.attrs)?;

        if attrs.block {
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
        } else {
            self.flush_run()?;
            let code_run = Run::new().add_text(attrs.text).style("CodeInline");
            if let Some(ref mut para) = self.current_paragraph {
                *para = para.clone().add_run(code_run);
            }
            self.current_run = Some(Run::new());
        }

        Ok(())
    }

    fn process_table(&mut self, root: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

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

        Ok(())
    }

    fn process_table_row(&mut self, row_element: &HtmlElement) -> Result<TableRow> {
        // let mut row = TableRow::new(vec![]);
        let mut cells = Vec::new();
        for child in &row_element.children {
            if let HtmlNode::Element(cell_element) = child {
                let cell = self.process_table_cell(cell_element)?;
                // row = row.add_cell(cell);
                cells.push(cell);
            }
        }
        let row = TableRow::new(cells).cant_split();
        Ok(row)
    }

    fn process_table_cell(&mut self, cell_element: &HtmlElement) -> Result<TableCell> {
        let prev_paragraph = self.current_paragraph.take();
        let prev_run = self.current_run.take();

        self.current_paragraph = Some(Paragraph::new());
        self.current_run = Some(Run::new());
        self.text_buffer.clear();

        self.convert_children(cell_element)?;
        self.flush_run()?;

        let cell_paragraph = self.current_paragraph.take().unwrap_or_default();

        self.current_paragraph = prev_paragraph;
        self.current_run = prev_run;

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
