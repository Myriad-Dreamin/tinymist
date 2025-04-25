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

/// DOCX Converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
    pub list_level: usize,
    docx: Docx,
    current_paragraph: Option<Paragraph>,
    current_run: Option<Run>,
    text_buffer: String,
    styles: DocxStyles,
    numbering: DocxNumbering,
    numbered_levels: Vec<(usize, bool)>,
    current_paragraph_has_content: bool,
    current_ordered_id: usize,
    current_unordered_id: usize,
}

impl DocxConverter {
    /// Create a new DOCX converter
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
            list_level: 0,
            docx: Docx::new(),
            current_paragraph: Some(Paragraph::new()),
            current_run: Some(Run::new()),
            text_buffer: String::new(),
            styles: DocxStyles::new(),
            numbering: DocxNumbering::new(),
            numbered_levels: Vec::new(),
            current_paragraph_has_content: false,
            // Initialize to 0, indicating no numbering has been created yet
            current_ordered_id: 0,
            current_unordered_id: 0,
        }
    }

    /// Convert HTML element to DOCX format
    pub fn convert(&mut self, root: &HtmlElement) -> Result<()> {
        self.initialize_document();
        self.process_element(root)
    }

    /// Process an HTML element
    fn process_element(&mut self, element: &HtmlElement) -> Result<()> {
        match element.tag {
            tag::head => Ok(()),

            tag::html | tag::body | md_tag::doc => {
                for child in &element.children {
                    if let HtmlNode::Element(child_elem) = child {
                        self.process_element(child_elem)?;
                    }
                }
                Ok(())
            }

            md_tag::parbreak => {
                self.flush_paragraph()?;
                Ok(())
            }

            md_tag::linebreak => {
                self.add_line_break();
                Ok(())
            }

            md_tag::heading => self.process_heading(element),

            tag::ol => self.process_ordered_list(element),
            tag::ul => self.process_unordered_list(element),
            tag::li => self.process_list_item(element),

            tag::p | tag::span => {
                self.process_children(element)?;
                Ok(())
            }

            tag::dl | tag::dt | tag::dd => {
                self.process_children(element)?;
                Ok(())
            }

            tag::strong | md_tag::strong => self.process_strong(element),
            tag::em | md_tag::emph => self.process_emphasis(element),
            md_tag::highlight => self.process_highlight(element),
            md_tag::strike => self.process_strike(element),

            md_tag::raw => self.process_raw(element),

            md_tag::label | md_tag::reference | md_tag::outline | md_tag::outline_entry => {
                self.process_inline_code(element)
            }

            md_tag::quote => self.process_quote(element),

            md_tag::table | md_tag::grid => self.process_table(element),
            md_tag::table_cell | md_tag::grid_cell => {
                self.process_children(element)?;
                Ok(())
            }

            md_tag::link => self.process_link(element),

            md_tag::image => self.process_image(element),

            md_tag::math_equation_inline => self.process_math_inline(element),
            md_tag::math_equation_block => self.process_math_block(element),

            tag::div | tag::figure => {
                self.flush_run()?;
                self.process_children(element)?;
                Ok(())
            }

            tag::figcaption => self.process_caption(element),

            tag::pre => self.process_pre(element),

            _ => self.process_unknown(element),
        }
    }

    /// Flush current run to paragraph, only process if there is content
    fn flush_run(&mut self) -> Result<()> {
        if !self.text_buffer.is_empty() {
            if let Some(ref mut run) = self.current_run {
                *run = run.clone().add_text(&self.text_buffer);
                self.current_paragraph_has_content = true;
            }
            self.text_buffer.clear();
        }

        if let (Some(ref mut para), Some(run)) =
            (&mut self.current_paragraph, self.current_run.take())
        {
            if !run.children.is_empty() {
                *para = para.clone().add_run(run);
                self.current_paragraph_has_content = true;
            }
        }

        self.current_run = Some(Run::new());
        Ok(())
    }

    /// Flush current paragraph to document, only add if paragraph actually contains content
    fn flush_paragraph(&mut self) -> Result<()> {
        self.flush_run()?;

        if let Some(para) = self.current_paragraph.take() {
            if self.current_paragraph_has_content {
                self.docx = self.docx.clone().add_paragraph(para);
            }
        }

        self.current_paragraph = Some(Paragraph::new());
        self.current_paragraph_has_content = false;
        Ok(())
    }

    /// Add text to buffer
    fn add_text(&mut self, text: &str) {
        self.text_buffer.push_str(text);
        self.current_paragraph_has_content = true;
    }

    /// Add line break
    fn add_line_break(&mut self) {
        self.flush_run().ok();
        if let Some(ref mut run) = self.current_run {
            *run = run.clone().add_break(BreakType::TextWrapping);
        }
    }

    /// Process child elements
    fn process_children(&mut self, element: &HtmlElement) -> Result<()> {
        for child in &element.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => self.process_frame(frame)?,
                HtmlNode::Text(text, _) => {
                    self.add_text(text);
                }
                HtmlNode::Element(element) => {
                    self.process_element(element)?;
                }
            }
        }
        Ok(())
    }

    /// Initialize document
    fn initialize_document(&mut self) {
        self.docx = self.styles.initialize_styles(self.docx.clone());
        self.docx = self.numbering.initialize_numbering(self.docx.clone());
    }

    /// Process frame element
    fn process_frame(&mut self, frame: &Frame) -> Result<()> {
        self.flush_run()?;

        let png_data = self.render_frame_to_png(frame)?;
        let (width, height) = self.calculate_image_dimensions(&png_data, Some(96.0 / 300.0 / 2.0));
        let pic = Pic::new(&png_data).size(width, height);

        if let Some(run) = self.current_run.take() {
            self.current_run = Some(run.add_image(pic));
        }

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

    /// Process heading
    fn process_heading(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

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

        let heading_para = Paragraph::new().style(style_name);
        self.current_paragraph = Some(heading_para);

        self.process_children(element)?;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Create and process list
    fn process_list(&mut self, element: &HtmlElement, is_ordered: bool) -> Result<()> {
        self.flush_paragraph()?;

        let prev_state = self.list_state;
        let prev_level = self.list_level;

        if self.list_level == 0 {
            let (docx, numbering_id) = if is_ordered {
                self.numbering.create_ordered_numbering(self.docx.clone())
            } else {
                self.numbering.create_unordered_numbering(self.docx.clone())
            };
            self.docx = docx;
            if is_ordered {
                self.current_ordered_id = numbering_id;
            } else {
                self.current_unordered_id = numbering_id;
            }
        }

        self.list_state = Some(if is_ordered {
            ListState::Ordered
        } else {
            ListState::Unordered
        });
        self.list_level = prev_level + 1;

        self.numbered_levels
            .retain(|(level, _)| *level != self.list_level);
        self.numbered_levels.push((self.list_level, is_ordered));

        self.process_children(element)?;

        self.numbered_levels
            .retain(|(level, _)| *level < self.list_level);
        self.list_level = prev_level;
        self.list_state = prev_state;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Process ordered list
    fn process_ordered_list(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_list(element, true)
    }

    /// Process unordered list
    fn process_unordered_list(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_list(element, false)
    }

    /// Process list item
    fn process_list_item(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_run()?;

        let mut paragraph = Paragraph::new();

        if let Some(list_state) = self.list_state {
            let is_ordered = matches!(list_state, ListState::Ordered);
            let numbering_id = if is_ordered {
                self.current_ordered_id
            } else {
                self.current_unordered_id
            };

            let level = IndentLevel::new(self.list_level.saturating_sub(1));
            paragraph = paragraph.numbering(NumberingId::new(numbering_id), level);
        }

        self.current_paragraph = Some(paragraph);

        self.process_children(element)?;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Process bold text
    fn process_strong(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_styled_text(element, "Strong")
    }

    /// Process italic text
    fn process_emphasis(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_styled_text(element, "Emphasis")
    }

    /// Process highlighted text
    fn process_highlight(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_styled_text(element, "Highlight")
    }

    /// Process strikethrough text
    fn process_strike(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_run()?;

        let prev_run = self.current_run.take();
        self.current_run = Some(Run::new().strike());

        self.process_children(element)?;
        self.flush_run()?;

        self.current_run = prev_run;

        Ok(())
    }

    /// Generic text style processing logic
    fn process_styled_text(&mut self, element: &HtmlElement, style_name: &str) -> Result<()> {
        self.flush_run()?;

        let prev_run = self.current_run.take();
        self.current_run = Some(Run::new().style(style_name));

        self.process_children(element)?;
        self.flush_run()?;

        self.current_run = prev_run;

        Ok(())
    }

    /// Process inline code
    fn process_inline_code(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_styled_text(element, "CodeInline")
    }

    /// Process blockquote
    fn process_quote(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

        let quote_para = Paragraph::new().style("Blockquote");
        self.current_paragraph = Some(quote_para);

        self.process_children(element)?;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Process link
    fn process_link(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = LinkAttr::parse(&element.attrs)?;

        self.flush_run()?;

        let prev_run = self.current_run.take();
        self.current_run = Some(Run::new().style("Hyperlink"));

        self.process_children(element)?;
        self.flush_run()?;

        if let Some(ref mut para) = self.current_paragraph {
            if let Some(ParagraphChild::Run(run)) = para.children.last().cloned() {
                para.children.pop();

                let hyperlink = Hyperlink::new(&attrs.dest, HyperlinkType::External).add_run(*run);
                *para = para.clone().add_hyperlink(hyperlink);
            }
        }

        self.current_run = prev_run;

        Ok(())
    }

    /// Process image
    fn process_image(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = ImageAttr::parse(&element.attrs)?;
        let src = unix_slash(Path::new(attrs.src.as_str()));

        self.flush_paragraph()?;

        if let Ok(img_data) = std::fs::read(&src) {
            let (width, height) = self.calculate_image_dimensions(&img_data, None);
            let pic = Pic::new(&img_data).size(width, height);

            let img_para = Paragraph::new().add_run(Run::new().add_image(pic));
            self.docx = self.docx.clone().add_paragraph(img_para);

            if !attrs.alt.is_empty() {
                let caption_para = Paragraph::new()
                    .style("Caption")
                    .add_run(Run::new().add_text(&attrs.alt));
                self.docx = self.docx.clone().add_paragraph(caption_para);
            }
        } else {
            let placeholder_para =
                Paragraph::new().add_run(Run::new().add_text(format!("[Image: {}]", attrs.alt)));
            self.docx = self.docx.clone().add_paragraph(placeholder_para);
        }

        Ok(())
    }

    /// Process raw code block
    fn process_raw(&mut self, element: &HtmlElement) -> Result<()> {
        let attrs = RawAttr::parse(&element.attrs)?;

        if attrs.block {
            self.flush_paragraph()?;

            let lang_para = Paragraph::new().style("CodeBlock");
            self.current_paragraph = Some(lang_para);
            self.add_text(&attrs.lang);
            self.flush_paragraph()?;

            let raw_para = Paragraph::new().style("CodeBlock");
            self.current_paragraph = Some(raw_para);
            let lines: Vec<&str> = attrs.text.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                self.add_text(line);
                if i < lines.len() - 1 {
                    self.add_line_break();
                }
            }
            self.flush_paragraph()?;
        } else {
            self.flush_run()?;

            let prev_run = self.current_run.take();
            self.current_run = Some(Run::new().style("CodeInline").add_text(&attrs.text));
            self.flush_run()?;

            self.current_run = prev_run;
        }

        Ok(())
    }

    /// Process inline math equation
    fn process_math_inline(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_math(element, false)
    }

    /// Process block math equation
    fn process_math_block(&mut self, element: &HtmlElement) -> Result<()> {
        self.process_math(element, true)
    }

    /// Generic math processing logic
    fn process_math(&mut self, element: &HtmlElement, is_block: bool) -> Result<()> {
        if is_block {
            self.flush_paragraph()?;
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

            let (width, height) =
                self.calculate_image_dimensions(&png_data, Some(96.0 / 300.0 / 2.0));
            let pic = Pic::new(&png_data).size(width, height);

            if is_block {
                let math_para = Paragraph::new()
                    .style("MathBlock")
                    .add_run(Run::new().add_image(pic));

                self.docx = self.docx.clone().add_paragraph(math_para);
            } else {
                self.flush_run()?;
                if let Some(run) = self.current_run.take() {
                    self.current_run = Some(run.add_image(pic));
                }
                self.flush_run()?;
            }
        } else if is_block {
            let placeholder_para = Paragraph::new()
                .style("MathBlock")
                .add_run(Run::new().add_text("[Math Expression]"));

            self.docx = self.docx.clone().add_paragraph(placeholder_para);
        } else {
            self.add_text("[Math Expression]");
        }

        Ok(())
    }

    /// Process image caption
    fn process_caption(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

        let caption_para = Paragraph::new().style("Caption");
        self.current_paragraph = Some(caption_para);

        self.process_children(element)?;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Process preformatted block
    fn process_pre(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

        let pre_para = Paragraph::new().style("CodeBlock");
        self.current_paragraph = Some(pre_para);

        self.process_children(element)?;
        self.flush_paragraph()?;

        Ok(())
    }

    /// Process table
    fn process_table(&mut self, element: &HtmlElement) -> Result<()> {
        self.flush_paragraph()?;

        let mut rows = Vec::new();

        for child in &element.children {
            if let HtmlNode::Element(row_elem) = child {
                let mut cells = Vec::new();

                for cell_node in &row_elem.children {
                    if let HtmlNode::Element(cell_elem) = cell_node {
                        if cell_elem.tag == md_tag::table_cell || cell_elem.tag == md_tag::grid_cell
                        {
                            let cell_para = Paragraph::new();
                            self.current_paragraph = Some(cell_para);

                            self.process_children(cell_elem)?;
                            self.flush_run()?;

                            if let Some(para) = self.current_paragraph.take() {
                                let cell = TableCell::new().add_paragraph(para);
                                cells.push(cell);
                            }

                            self.current_paragraph = Some(Paragraph::new());
                        }
                    }
                }

                if !cells.is_empty() {
                    rows.push(TableRow::new(cells).cant_split());
                }
            }
        }

        if !rows.is_empty() {
            let table = Table::new(vec![]).style("Table");
            let table_with_rows = rows.into_iter().fold(table, |t, r| t.add_row(r));
            self.docx = self.docx.clone().add_table(table_with_rows);
        }

        Ok(())
    }

    /// Process unknown tag
    fn process_unknown(&mut self, element: &HtmlElement) -> Result<()> {
        self.add_text(&format!("[Unknown tag: {:?}]", element.tag));
        self.process_children(element)?;
        Ok(())
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
        self.flush_paragraph()?;

        let docx = self.docx.clone().build();
        let mut buffer = Vec::new();
        docx.pack(&mut std::io::Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;

        Ok(buffer)
    }
}
