//! List numbering management for DOCX conversion

use docx_rs::*;

/// List numbering management for DOCX
#[derive(Clone, Debug)]
pub struct DocxNumbering {
    initialized: bool,
    next_id: usize,
}

impl DocxNumbering {
    /// Create a new numbering manager
    pub fn new() -> Self {
        Self {
            initialized: false,
            next_id: 1,
        }
    }

    /// Create a list level with the specified parameters
    pub fn create_list_level(id: usize, format: &str, text: &str, is_bullet: bool) -> Level {
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

    /// Initialize the numbering manager
    pub fn initialize_numbering(&mut self, docx: Docx) -> Docx {
        if self.initialized {
            return docx;
        }

        self.initialized = true;
        docx
    }

    /// Create a new ordered list numbering, including a new AbstractNumbering instance
    pub fn create_ordered_numbering(&mut self, docx: Docx) -> (Docx, usize) {
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
    pub fn create_unordered_numbering(&mut self, docx: Docx) -> (Docx, usize) {
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
