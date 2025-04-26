//! Core functionality for converting HTML to DOCX format

use typst::html::HtmlElement;

use crate::converter::{FormatWriter, HtmlToAstParser};
use crate::Result;
use crate::TypliteFeat;

use super::DocxWriter;

/// DOCX Converter implementation
#[derive(Clone, Debug)]
pub struct DocxConverter {
    pub feat: TypliteFeat,
}

impl DocxConverter {
    /// Create a new DOCX converter
    pub fn new(feat: TypliteFeat) -> Self {
        Self { feat }
    }

    /// Convert HTML element to DOCX format
    pub fn convert(&mut self, root: &HtmlElement) -> Result<Vec<u8>> {
        // 使用共享解析器解析 HTML 到 AST
        let parser = HtmlToAstParser::new(self.feat.clone());
        let document = parser.parse(root)?;

        // 创建并初始化 DirectDocxWriter
        let mut writer = DocxWriter::new(self.feat.clone());

        // 使用 DirectDocxWriter 处理 AST
        writer.write_vec(&document)
    }
}
