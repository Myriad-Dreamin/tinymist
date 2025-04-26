//! Writer functionality for rendering DocxNode structure to DOCX format

use docx_rs::*;

use crate::Result;
use super::types::{DocxNode, DocxInline};
use super::utils::calculate_image_dimensions;
use super::styles::DocxStyles;
use super::numbering::DocxNumbering;

//------------------------------------------------------------------------------
// WRITER IMPLEMENTATION (DocxNode structure to DOCX)
//------------------------------------------------------------------------------

/// DOCX Writer: Responsible for rendering DocxNodes into a DOCX document
#[derive(Clone, Debug)]
pub struct DocxWriter {
    styles: DocxStyles,
}

impl DocxWriter {
    /// Create a new DOCX writer
    pub fn new() -> Self {
        Self {
            styles: DocxStyles::new(),
        }
    }

    /// Write DocxNodes to a DOCX document and return the byte content
    pub fn write(
        &mut self,
        nodes: &[DocxNode],
        list_numbering_ids: &[(bool, usize)],
    ) -> Result<Vec<u8>> {
        // Initialize DOCX document with styles
        let mut docx = Docx::new();
        docx = self.styles.initialize_styles(docx);

        // Create and initialize numbering definitions
        let mut numbering = DocxNumbering::new();
        for (is_ordered, _) in list_numbering_ids {
            let (new_docx, _) = if *is_ordered {
                numbering.create_ordered_numbering(docx)
            } else {
                numbering.create_unordered_numbering(docx)
            };
            docx = new_docx;
        }
        docx = numbering.initialize_numbering(docx);

        // Process nodes to build DOCX document
        docx = self.build_docx_document(docx, nodes);

        // Build and pack document
        let docx_built = docx.build();
        let mut buffer = Vec::new();
        docx_built
            .pack(&mut std::io::Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;

        Ok(buffer)
    }

    /// Build DOCX document from nodes - Internal writer function
    fn build_docx_document(&self, mut docx: Docx, nodes: &[DocxNode]) -> Docx {
        // Process all nodes to create the final DOCX document
        for node in nodes {
            docx = match node {
                DocxNode::Paragraph {
                    style,
                    content,
                    numbering,
                } => self.process_paragraph_node(docx, style, content, numbering),

                DocxNode::Table { rows } => self.process_table_node(docx, rows),

                DocxNode::Image { data, alt } => self.process_image_node(docx, data, alt),
            };
        }

        docx
    }

    /// Process paragraph node - Writer helper function
    fn process_paragraph_node(
        &self,
        docx: Docx,
        style: &Option<String>,
        content: &[DocxInline],
        numbering: &Option<(usize, usize)>,
    ) -> Docx {
        let mut para = Paragraph::new();

        // Apply style if specified
        if let Some(style_name) = style {
            para = para.style(style_name);
        }

        // Apply numbering if specified
        if let Some((numbering_id, level)) = numbering {
            para = para.numbering(NumberingId::new(*numbering_id), IndentLevel::new(*level));
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

    /// Process table node - Writer helper function
    fn process_table_node(&self, docx: Docx, rows: &[Vec<Vec<DocxNode>>]) -> Docx {
        let mut table = Table::new(vec![]).style("Table");

        for row_cells in rows {
            let mut cells = Vec::new();

            for cell_nodes in row_cells {
                let mut table_cell = TableCell::new();

                for cell_node in cell_nodes {
                    if let DocxNode::Paragraph {
                        style,
                        content,
                        numbering,
                    } = cell_node
                    {
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
                }

                cells.push(table_cell);
            }

            let table_row = TableRow::new(cells).cant_split();
            table = table.add_row(table_row);
        }

        docx.add_table(table)
    }

    /// Process image node - Writer helper function
    fn process_image_node(&self, docx: Docx, data: &[u8], alt: &str) -> Docx {
        let (width, height) = calculate_image_dimensions(data, None);
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
                DocxInline::Image { data } => {
                    let (width, height) =
                        calculate_image_dimensions(data, Some(96.0 / 300.0 / 2.0));
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
                DocxInline::Image { data } => {
                    let (width, height) =
                        calculate_image_dimensions(data, Some(96.0 / 300.0 / 2.0));
                    let pic = Pic::new(data).size(width, height);
                    run = run.add_image(pic);
                }
                DocxInline::Strong(nested)
                | DocxInline::Emphasis(nested)
                | DocxInline::Highlight(nested)
                | DocxInline::Strike(nested) => {
                    // 递归提取嵌套内容中的文本
                    for nested_inline in nested {
                        match nested_inline {
                            DocxInline::Text(text) => {
                                if !text.is_empty() {
                                    run = run.add_text(text);
                                }
                            }
                            DocxInline::LineBreak => {
                                run = run.add_break(BreakType::TextWrapping);
                            }
                            _ => {
                                // 二级嵌套元素以文本形式简单处理
                                run = run.add_text("[Nested content]");
                            }
                        }
                    }
                }
                DocxInline::Hyperlink {
                    url: _,
                    content: nested,
                } => {
                    // 提取超链接中的文本
                    for nested_inline in nested {
                        if let DocxInline::Text(text) = nested_inline {
                            if !text.is_empty() {
                                run = run.add_text(text);
                            }
                        }
                    }
                }
                DocxInline::Code(text) => {
                    if !text.is_empty() {
                        run = run.add_text(text);
                    }
                }
            }
        }

        run
    }
}