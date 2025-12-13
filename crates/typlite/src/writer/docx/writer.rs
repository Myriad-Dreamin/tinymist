//! DOCX document writer implementation

use docx_rs::*;
use ecow::EcoString;
use log::{debug, warn};
use std::fs;
use std::io::Cursor;

use crate::Result;
use crate::ir;
use crate::writer::IrFormatWriter;

use super::image_processor::DocxImageProcessor;
use super::numbering::DocxNumbering;
use super::styles::DocxStyles;

/// DOCX writer that generates DOCX directly from typlite IR.
pub struct DocxWriter {
    styles: DocxStyles,
    numbering: DocxNumbering,
    list_level: usize,
    list_numbering_count: usize,
    image_processor: DocxImageProcessor,
}

impl Default for DocxWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl DocxWriter {
    pub fn new() -> Self {
        Self {
            styles: DocxStyles::new(),
            numbering: DocxNumbering::new(),
            list_level: 0,
            list_numbering_count: 0,
            image_processor: DocxImageProcessor::new(),
        }
    }

    fn process_ir_inline_to_run(&mut self, mut run: Run, node: &ir::Inline) -> Result<Run> {
        match node {
            ir::Inline::Text(text) => {
                run = run.add_text(text);
            }
            ir::Inline::Strong(content) => {
                run = run.style("Strong");
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
            }
            ir::Inline::Emphasis(content) => {
                run = run.style("Emphasis");
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
            }
            ir::Inline::Strikethrough(content) => {
                run = run.strike();
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
            }
            ir::Inline::Group(content) => {
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
            }
            ir::Inline::Highlight(content) => {
                run = run.highlight("yellow");
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
            }
            ir::Inline::Link { content, .. } => {
                run = run.style("Hyperlink");
                for child in content {
                    run = self.process_ir_inline_to_run(run, child)?;
                }
                // `Hyperlink` nodes need to be attached at paragraph level. The paragraph
                // writer handles `Inline::Link` explicitly.
            }
            ir::Inline::ReferenceLink { label, content } => {
                if content.is_empty() {
                    run = run.add_text(label);
                } else {
                    for child in content {
                        run = self.process_ir_inline_to_run(run, child)?;
                    }
                }
            }
            ir::Inline::Image { url, .. } => {
                if let Ok(img_data) = fs::read(url.as_str()) {
                    run = self.image_processor.process_inline_image(run, &img_data)?;
                } else {
                    run = run.add_text(format!("[Image not found: {url}]"));
                }
            }
            ir::Inline::Autolink { url, .. } => {
                run = run.add_text(url);
            }
            ir::Inline::InlineCode(code) => {
                run = run.style("CodeInline").add_text(code);
            }
            ir::Inline::HardBreak => {
                run = run.add_break(BreakType::TextWrapping);
            }
            ir::Inline::SoftBreak => {
                run = run.add_text(" ");
            }
            ir::Inline::HtmlElement(element) => {
                if element.tag == "mark" {
                    run = run.highlight("yellow");
                    for child in &element.children {
                        if let ir::IrNode::Inline(inline) = child {
                            run = self.process_ir_inline_to_run(run, inline)?;
                        }
                    }
                } else if element.tag == "img" && element.self_closing {
                    let is_typst_block = element
                        .attributes
                        .iter()
                        .any(|a| a.name == "alt" && a.value == "typst-block");

                    let src = element
                        .attributes
                        .iter()
                        .find(|a| a.name == "src")
                        .map(|a| a.value.as_str())
                        .unwrap_or("");

                    if src.starts_with("data:image/") {
                        run = self.image_processor.process_data_url_image(
                            run,
                            src,
                            is_typst_block,
                        )?;
                    }
                } else {
                    for child in &element.children {
                        match child {
                            ir::IrNode::Inline(inline) => {
                                run = self.process_ir_inline_to_run(run, inline)?;
                            }
                            ir::IrNode::Block(block) => {
                                debug!(
                                    "unhandled block node inside inline HTML element: {block:?}"
                                );
                            }
                        }
                    }
                }
            }
            ir::Inline::EmbeddedBlock(block) => {
                debug!("ignoring embedded block inside paragraph for DOCX export: {block:?}");
            }
            ir::Inline::Verbatim(text) => {
                warn!("ignoring `m1verbatim` content in DOCX export: {text:?}");
            }
            ir::Inline::Comment(_) => {}
            ir::Inline::UnsupportedCustom => {}
        }

        Ok(run)
    }

    fn build_caption_paragraph_ir(
        &mut self,
        prefix: Option<&str>,
        nodes: &[ir::Inline],
    ) -> Result<Option<Paragraph>> {
        if nodes.is_empty() {
            return Ok(None);
        }

        let mut para = Paragraph::new().style("Caption");

        if let Some(prefix_text) = prefix {
            para = para.add_run(Run::new().add_text(prefix_text));
        }

        for node in nodes {
            let run = self.process_ir_inline_to_run(Run::new(), node)?;
            if !run.children.is_empty() {
                para = para.add_run(run);
            }
        }

        Ok(Some(para))
    }

    fn process_paragraph_ir(
        &mut self,
        mut docx: Docx,
        content: &[ir::Inline],
        style: Option<&str>,
    ) -> Result<Docx> {
        let mut para = Paragraph::new();
        if let Some(style_name) = style {
            para = para.style(style_name);
        }

        for node in content {
            match node {
                ir::Inline::Link { url, content, .. } => {
                    let mut hyperlink_run = Run::new().style("Hyperlink");
                    for child in content {
                        hyperlink_run = self.process_ir_inline_to_run(hyperlink_run, child)?;
                    }
                    if !hyperlink_run.children.is_empty() {
                        let hyperlink = Hyperlink::new(url.as_str(), HyperlinkType::External)
                            .add_run(hyperlink_run);
                        para = para.add_hyperlink(hyperlink);
                    }
                }
                other => {
                    let run = self.process_ir_inline_to_run(Run::new(), other)?;
                    if !run.children.is_empty() {
                        para = para.add_run(run);
                    }
                }
            }
        }

        if !para.children.is_empty() {
            docx = docx.add_paragraph(para);
        }
        Ok(docx)
    }

    fn process_ir_block(&mut self, mut docx: Docx, node: &ir::Block) -> Result<Docx> {
        match node {
            ir::Block::Document(blocks) => {
                for block in blocks {
                    docx = self.process_ir_block(docx, block)?;
                }
            }
            ir::Block::Paragraph(content) => {
                docx = self.process_paragraph_ir(docx, content, None)?;
            }
            ir::Block::Heading { level, content } => {
                let style_name = match *level {
                    1 => "Heading1",
                    2 => "Heading2",
                    3 => "Heading3",
                    4 => "Heading4",
                    5 => "Heading5",
                    _ => "Heading6",
                };
                docx = self.process_paragraph_ir(docx, content, Some(style_name))?;
            }
            ir::Block::BlockQuote(content) => {
                for block in content {
                    match block {
                        ir::Block::Paragraph(inline) => {
                            docx = self.process_paragraph_ir(docx, inline, Some("Blockquote"))?;
                        }
                        other => {
                            docx = self.process_ir_block(docx, other)?;
                        }
                    }
                }
            }
            ir::Block::CodeBlock {
                language, content, ..
            } => {
                if let Some(lang) = language
                    && !lang.is_empty()
                {
                    let lang_para = Paragraph::new()
                        .style("CodeBlock")
                        .add_run(Run::new().add_text(lang));
                    docx = docx.add_paragraph(lang_para);
                }

                for line in content.split('\n') {
                    let code_para = Paragraph::new()
                        .style("CodeBlock")
                        .add_run(Run::new().add_text(line));
                    docx = docx.add_paragraph(code_para);
                }
            }
            ir::Block::OrderedList { items, .. } => {
                docx = self.process_ir_ordered_list(docx, items)?;
            }
            ir::Block::UnorderedList(items) => {
                docx = self.process_ir_unordered_list(docx, items)?;
            }
            ir::Block::Table(table) => {
                docx = self.process_ir_table(docx, table)?;
            }
            ir::Block::Figure { body, caption } => {
                docx = self.process_ir_figure(docx, body, caption)?;
            }
            ir::Block::ExternalFrame(frame) => {
                if let Ok(png_data) = self
                    .image_processor
                    .convert_svg_to_png(frame.svg.as_bytes())
                {
                    docx = self.image_processor.process_image_data(
                        docx,
                        &png_data,
                        Some(frame.alt_text.as_str()),
                        None,
                    );
                } else if let Ok(img_data) = fs::read(&frame.file_path) {
                    docx = self.image_processor.process_image_data(
                        docx,
                        &img_data,
                        Some(frame.alt_text.as_str()),
                        None,
                    );
                }
            }
            ir::Block::Center(inner) => {
                match &**inner {
                    // Match the current (cmark-based) DOCX behavior: a centered paragraph
                    // (e.g. block equation) ends up as block-level HTML with inline children,
                    // which the DOCX writer currently drops.
                    ir::Block::Paragraph(_) => {}
                    other => {
                        let start_idx = docx.document.children.len();
                        docx = self.process_ir_block(docx, other)?;
                        for child in docx.document.children.iter_mut().skip(start_idx) {
                            if let DocumentChild::Paragraph(para) = child {
                                para.property = para.property.clone().align(AlignmentType::Center);
                            }
                        }
                    }
                }
            }
            ir::Block::Alert { content, .. } => {
                for block in content {
                    docx = self.process_ir_block(docx, block)?;
                }
            }
            ir::Block::ThematicBreak => {
                let hr_para = Paragraph::new()
                    .style("HorizontalLine")
                    .add_run(Run::new().add_text(""));
                docx = docx.add_paragraph(hr_para);
            }
            ir::Block::HtmlElement(element) => {
                // Keep behavior conservative: treat as container and only render nested blocks.
                for child in &element.children {
                    if let ir::IrNode::Block(block) = child {
                        docx = self.process_ir_block(docx, block)?;
                    }
                }
            }
            ir::Block::HtmlBlock(_) => {}
        }

        Ok(docx)
    }

    fn process_ir_ordered_list(&mut self, mut docx: Docx, items: &[ir::ListItem]) -> Result<Docx> {
        self.list_level += 1;
        let current_level = self.list_level - 1;

        let (doc, num_id) = self.numbering.create_ordered_numbering(docx);
        docx = doc;

        for item in items {
            if let ir::ListItem::Ordered { content, .. } = item {
                docx = self.process_ir_list_item_content(docx, content, num_id, current_level)?;
            }
        }

        self.list_level -= 1;
        Ok(docx)
    }

    fn process_ir_unordered_list(
        &mut self,
        mut docx: Docx,
        items: &[ir::ListItem],
    ) -> Result<Docx> {
        self.list_level += 1;
        let current_level = self.list_level - 1;

        let (doc, num_id) = self.numbering.create_unordered_numbering(docx);
        docx = doc;

        for item in items {
            if let ir::ListItem::Unordered { content } = item {
                docx = self.process_ir_list_item_content(docx, content, num_id, current_level)?;
            }
        }

        self.list_level -= 1;
        Ok(docx)
    }

    fn process_ir_list_item_content(
        &mut self,
        mut docx: Docx,
        content: &[ir::Block],
        num_id: usize,
        level: usize,
    ) -> Result<Docx> {
        if content.is_empty() {
            let empty_para = Paragraph::new()
                .numbering(NumberingId::new(num_id), IndentLevel::new(level))
                .add_run(Run::new().add_text(""));
            return Ok(docx.add_paragraph(empty_para));
        }

        for block in content {
            match block {
                ir::Block::Paragraph(inline) => {
                    let mut para = Paragraph::new()
                        .numbering(NumberingId::new(num_id), IndentLevel::new(level));

                    for node in inline {
                        let run = self.process_ir_inline_to_run(Run::new(), node)?;
                        if !run.children.is_empty() {
                            para = para.add_run(run);
                        }
                    }

                    docx = docx.add_paragraph(para);
                }
                ir::Block::OrderedList { .. } | ir::Block::UnorderedList(_) => {
                    docx = self.process_ir_block(docx, block)?;
                }
                _ => {
                    docx = self.process_ir_block(docx, block)?;
                }
            }
        }

        Ok(docx)
    }

    fn process_ir_table(&mut self, mut docx: Docx, table: &ir::Table) -> Result<Docx> {
        if table.rows.is_empty() || table.columns == 0 {
            return Ok(docx);
        }

        let mut out_table = Table::new(vec![]).style("Table");
        let mut vmerge = vec![0usize; table.columns];

        for row in &table.rows {
            let mut cells = Vec::new();
            let mut col_index = 0;
            let mut cell_iter = row.cells.iter();

            while col_index < table.columns {
                if vmerge[col_index] > 0 {
                    cells.push(TableCell::new().vertical_merge(VMergeType::Continue));
                    vmerge[col_index] -= 1;
                    col_index += 1;
                    continue;
                }

                if let Some(cell) = cell_iter.next() {
                    let mut effective_align = cell.align.clone();
                    if effective_align.is_none() {
                        if let Some(column_align) = table.alignments.get(col_index).cloned() {
                            if !matches!(column_align, ir::TableAlignment::None) {
                                effective_align = Some(column_align);
                            }
                        }
                    }

                    let mut table_cell = self.build_table_cell_ir(cell, effective_align)?;
                    if cell.colspan > 1 {
                        table_cell = table_cell.grid_span(cell.colspan);
                    }
                    if cell.rowspan > 1 {
                        table_cell = table_cell.vertical_merge(VMergeType::Restart);
                        for offset in 0..cell.colspan {
                            if col_index + offset < table.columns {
                                vmerge[col_index + offset] =
                                    vmerge[col_index + offset].max(cell.rowspan - 1);
                            }
                        }
                    }
                    cells.push(table_cell);
                    col_index += cell.colspan;
                } else {
                    cells.push(TableCell::new());
                    col_index += 1;
                }
            }

            out_table = out_table.add_row(TableRow::new(cells));
        }

        docx = docx.add_table(out_table);
        Ok(docx)
    }

    fn build_table_cell_ir(
        &mut self,
        cell: &ir::TableCell,
        align: Option<ir::TableAlignment>,
    ) -> Result<TableCell> {
        let mut table_cell = TableCell::new();
        let mut para = Paragraph::new();

        let mut run = Run::new();
        for node in &cell.content {
            match node {
                ir::IrNode::Inline(inline) => {
                    run = self.process_ir_inline_to_run(run, inline)?;
                }
                ir::IrNode::Block(block) => {
                    debug!("ignoring block node inside table cell for DOCX export: {block:?}");
                }
            }
        }
        if !run.children.is_empty() {
            para = para.add_run(run);
        }

        if let Some(alignment) = align.and_then(|a| match a {
            ir::TableAlignment::Left => Some(AlignmentType::Left),
            ir::TableAlignment::Center => Some(AlignmentType::Center),
            ir::TableAlignment::Right => Some(AlignmentType::Right),
            ir::TableAlignment::None => None,
        }) {
            para.property = para.property.clone().align(alignment);
        }

        if !para.children.is_empty() {
            table_cell = table_cell.add_paragraph(para);
        }

        Ok(table_cell)
    }

    fn process_ir_figure(
        &mut self,
        mut docx: Docx,
        body: &ir::Block,
        caption: &[ir::Inline],
    ) -> Result<Docx> {
        match body {
            ir::Block::Paragraph(inlines) => {
                for inline in inlines {
                    match inline {
                        ir::Inline::Image { url, .. } => {
                            if let Ok(img_data) = fs::read(url.as_str()) {
                                docx = self
                                    .image_processor
                                    .process_image_data(docx, &img_data, None, None);

                                if let Some(caption_para) =
                                    self.build_caption_paragraph_ir(Some("Figure: "), caption)?
                                {
                                    docx = docx.add_paragraph(caption_para);
                                }
                            } else {
                                let placeholder = format!("[Image not found: {url}]");
                                let para =
                                    Paragraph::new().add_run(Run::new().add_text(placeholder));
                                docx = docx.add_paragraph(para);

                                if let Some(caption_para) =
                                    self.build_caption_paragraph_ir(None, caption)?
                                {
                                    docx = docx.add_paragraph(caption_para);
                                }
                            }
                        }
                        other => {
                            let mut para = Paragraph::new();
                            let run = self.process_ir_inline_to_run(Run::new(), other)?;
                            if !run.children.is_empty() {
                                para = para.add_run(run);
                                docx = docx.add_paragraph(para);
                            }
                        }
                    }
                }
            }
            other => {
                docx = self.process_ir_block(docx, other)?;
                if let Some(caption_para) = self.build_caption_paragraph_ir(None, caption)? {
                    docx = docx.add_paragraph(caption_para);
                }
            }
        }
        Ok(docx)
    }

    pub fn generate_docx_ir(&mut self, doc: &ir::Document) -> Result<Vec<u8>> {
        let mut docx = Docx::new();
        docx = self.styles.initialize_styles(docx);

        for block in &doc.blocks {
            docx = self.process_ir_block(docx, block)?;
        }

        docx = self.numbering.initialize_numbering(docx);

        let docx_built = docx.build();
        let mut buffer = Vec::new();
        docx_built
            .pack(&mut Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {e}"))?;

        Ok(buffer)
    }
}

impl IrFormatWriter for DocxWriter {
    fn write_ir_vec(&mut self, document: &ir::Document) -> Result<Vec<u8>> {
        self.list_level = 0;
        self.list_numbering_count = 0;
        self.generate_docx_ir(document)
    }

    fn write_ir_eco(&mut self, _document: &ir::Document, _output: &mut EcoString) -> Result<()> {
        Err("DOCX format does not support EcoString output".into())
    }
}
