//! DOCX document writer implementation

use base64::Engine;
use cmark_writer::ast::{
    ListItem, Node, TableAlignment, TableCell as AstTableCell, TableRow as AstTableRow,
};
use docx_rs::*;
use ecow::EcoString;
use log::{debug, warn};
use std::fs;
use std::io::Cursor;

use crate::Result;
use crate::common::{
    CenterNode, FigureNode, FormatWriter, HighlightNode, InlineNode, VerbatimNode,
};
use crate::ir;
use crate::writer::IrFormatWriter;

use super::image_processor::DocxImageProcessor;
use super::numbering::DocxNumbering;
use super::styles::DocxStyles;

/// DOCX writer that generates DOCX directly from AST (without intermediate
/// representation)
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

    /// Process image node
    fn process_image(&self, docx: Docx, url: &str, alt_nodes: &[Node]) -> Result<Docx> {
        // Build alt text
        let alt_text = if !alt_nodes.is_empty() {
            let mut text = String::new();
            for node in alt_nodes {
                if let Node::Text(content) = node {
                    text.push_str(content);
                }
            }
            Some(text)
        } else {
            None
        };

        // Try reading image file
        if let Ok(img_data) = fs::read(url) {
            Ok(self
                .image_processor
                .process_image_data(docx, &img_data, alt_text.as_deref(), None))
        } else {
            let placeholder = format!("[Image not found: {url}]");
            let para = Paragraph::new().add_run(Run::new().add_text(placeholder));
            Ok(docx.add_paragraph(para))
        }
    }

    /// Process figure node (image with caption)
    fn process_figure(&mut self, mut docx: Docx, figure_node: &FigureNode) -> Result<Docx> {
        // First handle the figure body (typically an image)
        match &*figure_node.body {
            Node::Paragraph(content) => {
                for node in content {
                    if let Node::Image {
                        url,
                        title: _,
                        alt: _,
                    } = node
                    {
                        // Process the image
                        if let Ok(img_data) = fs::read(url.as_str()) {
                            // Add the image with caption
                            docx = self
                                .image_processor
                                .process_image_data(docx, &img_data, None, None);

                            if let Some(caption_para) = self
                                .build_caption_paragraph(Some("Figure: "), &figure_node.caption)?
                            {
                                docx = docx.add_paragraph(caption_para);
                            }
                        } else {
                            // Image not found, show placeholder
                            let placeholder = format!("[Image not found: {url}]");
                            let para = Paragraph::new().add_run(Run::new().add_text(placeholder));
                            docx = docx.add_paragraph(para);

                            // Still add caption
                            if let Some(caption_para) =
                                self.build_caption_paragraph(None, &figure_node.caption)?
                            {
                                docx = docx.add_paragraph(caption_para);
                            }
                        }
                    } else {
                        // Handle non-image content
                        let mut para = Paragraph::new();
                        let run = Run::new();
                        let run = self.process_inline_to_run(run, node)?;
                        if !run.children.is_empty() {
                            para = para.add_run(run);
                            docx = docx.add_paragraph(para);
                        }

                        if let Some(caption_para) =
                            self.build_caption_paragraph(None, &figure_node.caption)?
                        {
                            docx = docx.add_paragraph(caption_para);
                        }
                    }
                }
            }
            // Handle other content types within figure
            _ => {
                // Process the content using standard node processing
                docx = self.process_node(docx, &figure_node.body)?;

                if let Some(caption_para) =
                    self.build_caption_paragraph(None, &figure_node.caption)?
                {
                    docx = docx.add_paragraph(caption_para);
                }
            }
        }

        Ok(docx)
    }

    /// Process inline element and add to Run
    fn process_inline_to_run(&self, mut run: Run, node: &Node) -> Result<Run> {
        match node {
            Node::Text(text) => {
                run = run.add_text(text);
            }
            Node::Strong(content) => {
                run = run.style("Strong");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Emphasis(content) => {
                run = run.style("Emphasis");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Strikethrough(content) => {
                run = run.strike();
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Link {
                url: _,
                title: _,
                content,
            } => {
                // Hyperlinks need to be processed at paragraph level, only handle content here
                run = run.style("Hyperlink");
                for child in content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            Node::Image {
                url,
                title: _,
                alt: _,
            } => {
                if let Ok(img_data) = fs::read(url.as_str()) {
                    run = self.image_processor.process_inline_image(run, &img_data)?;
                } else {
                    run = run.add_text(format!("[Image not found: {url}]"));
                }
            }
            Node::HtmlElement(element) => {
                // Handle special HTML elements
                if element.tag == "mark" {
                    run = run.highlight("yellow");
                    for child in &element.children {
                        run = self.process_inline_to_run(run, child)?;
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
                    // Standard element content processing
                    for child in &element.children {
                        run = self.process_inline_to_run(run, child)?;
                    }
                }
            }
            Node::InlineCode(code) => {
                run = run.style("CodeInline").add_text(code);
            }
            Node::HardBreak => {
                run = run.add_break(BreakType::TextWrapping);
            }
            Node::SoftBreak => {
                run = run.add_text(" ");
            }
            node if node.is_custom_type::<HighlightNode>() => {
                let highlight_node = node.as_custom_type::<HighlightNode>().unwrap();
                run = run.highlight("yellow");
                for child in &highlight_node.content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            node if node.is_custom_type::<InlineNode>() => {
                let inline_node = node.as_custom_type::<InlineNode>().unwrap();
                for child in &inline_node.content {
                    run = self.process_inline_to_run(run, child)?;
                }
            }
            node if node.is_custom_type::<VerbatimNode>() => {
                let node = node.as_custom_type::<VerbatimNode>().unwrap();
                warn!(
                    "ignoring `m1verbatim` content in DOCX export: {:?}",
                    node.content
                );
            }
            // Other inline element types
            _ => {
                debug!("unhandled inline node in DOCX export: {node:?}");
            }
        }

        Ok(run)
    }

    fn build_caption_paragraph(
        &self,
        prefix: Option<&str>,
        nodes: &[Node],
    ) -> Result<Option<Paragraph>> {
        if nodes.is_empty() {
            return Ok(None);
        }

        let mut para = Paragraph::new().style("Caption");

        if let Some(prefix_text) = prefix {
            para = para.add_run(Run::new().add_text(prefix_text));
        }

        for node in nodes {
            let run = self.process_inline_to_run(Run::new(), node)?;
            if !run.children.is_empty() {
                para = para.add_run(run);
            }
        }

        Ok(Some(para))
    }

    /// Process paragraph and add to document
    fn process_paragraph(
        &self,
        mut docx: Docx,
        content: &[Node],
        style: Option<&str>,
    ) -> Result<Docx> {
        let mut para = Paragraph::new();

        // Apply style
        if let Some(style_name) = style {
            para = para.style(style_name);
        }

        // Extract all link nodes
        let mut links = Vec::new();
        for (i, node) in content.iter().enumerate() {
            if let Node::Link {
                url,
                title: _,
                content: _,
            } = node
            {
                links.push((i, url.clone()));
            }
        }

        // If no links, process paragraph normally
        if links.is_empty() {
            // Process paragraph content
            for node in content {
                let run = Run::new();
                let run = self.process_inline_to_run(run, node)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }
            }
        } else {
            // If links exist, we need to process in segments
            let mut last_idx = 0;
            for (idx, url) in links {
                // Process content before the link
                for item in content.iter().take(idx).skip(last_idx) {
                    let run = Run::new();
                    let run = self.process_inline_to_run(run, item)?;
                    if !run.children.is_empty() {
                        para = para.add_run(run);
                    }
                }

                // Process link
                if let Node::Link {
                    url: _,
                    title: _,
                    content: link_content,
                } = &content[idx]
                {
                    let mut hyperlink_run = Run::new().style("Hyperlink");
                    for child in link_content {
                        hyperlink_run = self.process_inline_to_run(hyperlink_run, child)?;
                    }

                    // Create and add hyperlink
                    if !hyperlink_run.children.is_empty() {
                        let hyperlink =
                            Hyperlink::new(&url, HyperlinkType::External).add_run(hyperlink_run);
                        para = para.add_hyperlink(hyperlink);
                    }
                }

                last_idx = idx + 1;
            }

            // Process content after the last link
            for item in content.iter().skip(last_idx) {
                let run = Run::new();
                let run = self.process_inline_to_run(run, item)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }
            }
        }

        // Only add when paragraph has content
        if !para.children.is_empty() {
            docx = docx.add_paragraph(para);
        }

        Ok(docx)
    }

    /// Process node and add to document
    fn process_node(&mut self, mut docx: Docx, node: &Node) -> Result<Docx> {
        match node {
            Node::Document(blocks) => {
                for block in blocks {
                    docx = self.process_node(docx, block)?;
                }
            }
            Node::Paragraph(content) => {
                docx = self.process_paragraph(docx, content, None)?;
            }
            Node::Heading {
                level,
                content,
                heading_type: _,
            } => {
                // Determine heading style name
                let style_name = match level {
                    1 => "Heading1",
                    2 => "Heading2",
                    3 => "Heading3",
                    4 => "Heading4",
                    5 => "Heading5",
                    _ => "Heading6",
                };

                docx = self.process_paragraph(docx, content, Some(style_name))?;
            }
            Node::BlockQuote(content) => {
                for block in content {
                    if let Node::Paragraph(inline) = block {
                        docx = self.process_paragraph(docx, inline, Some("Blockquote"))?;
                    } else {
                        docx = self.process_node(docx, block)?;
                    }
                }
            }
            Node::CodeBlock {
                language,
                content,
                block_type: _,
            } => {
                // Add language information
                if let Some(lang) = language
                    && !lang.is_empty()
                {
                    let lang_para = Paragraph::new()
                        .style("CodeBlock")
                        .add_run(Run::new().add_text(lang));
                    docx = docx.add_paragraph(lang_para);
                }

                // Process code line by line, preserving line breaks
                let lines: Vec<&str> = content.split('\n').collect();
                for line in lines {
                    let code_para = Paragraph::new()
                        .style("CodeBlock")
                        .add_run(Run::new().add_text(line));
                    docx = docx.add_paragraph(code_para);
                }
            }
            Node::OrderedList { start: _, items } => {
                docx = self.process_ordered_list(docx, items)?;
            }
            Node::UnorderedList(items) => {
                docx = self.process_unordered_list(docx, items)?;
            }
            Node::Table {
                columns,
                rows,
                alignments,
            } => {
                docx = self.process_table(docx, *columns, rows, alignments)?;
            }
            Node::Image { url, title: _, alt } => {
                docx = self.process_image(docx, url, alt)?;
            }
            Node::HtmlElement(element) => {
                docx = self.process_html_element_block(docx, element)?;
            }
            node if node.is_custom_type::<FigureNode>() => {
                let figure_node = node.as_custom_type::<FigureNode>().unwrap();
                docx = self.process_figure(docx, figure_node)?;
            }
            node if node.is_custom_type::<CenterNode>() => {
                let center_node = node.as_custom_type::<CenterNode>().unwrap();
                // Handle regular node but with center alignment
                match &center_node.node {
                    Node::Paragraph(content) => {
                        docx = self.process_paragraph(docx, content, None)?;
                        // Get the last paragraph and center it
                        if let Some(DocumentChild::Paragraph(para)) =
                            docx.document.children.last_mut()
                        {
                            para.property = para.property.clone().align(AlignmentType::Center);
                        }
                    }
                    Node::HtmlElement(element) => {
                        let start_idx = docx.document.children.len();
                        for child in &element.children {
                            docx = self.process_node(docx, child)?;
                        }
                        for child in docx.document.children.iter_mut().skip(start_idx) {
                            if let DocumentChild::Paragraph(para) = child {
                                para.property = para.property.clone().align(AlignmentType::Center);
                            }
                        }
                    }
                    other => {
                        docx = self.process_node(docx, other)?;
                        // Get the last element and center it if it's a paragraph
                        if let Some(DocumentChild::Paragraph(para)) =
                            docx.document.children.last_mut()
                        {
                            para.property = para.property.clone().align(AlignmentType::Center);
                        }
                    }
                }
            }
            node if node.is_custom_type::<crate::common::ExternalFrameNode>() => {
                let external_frame = node
                    .as_custom_type::<crate::common::ExternalFrameNode>()
                    .unwrap();
                let data = base64::engine::general_purpose::STANDARD
                    .decode(&external_frame.svg)
                    .map_err(|e| format!("Failed to decode SVG data: {e}"))?;

                docx = self.image_processor.process_image_data(
                    docx,
                    &data,
                    Some(&external_frame.alt_text),
                    None,
                );
            }
            node if node.is_custom_type::<HighlightNode>() => {
                let highlight_node = node.as_custom_type::<HighlightNode>().unwrap();
                // Handle HighlightNode at block level (convert to paragraph)
                let mut para = Paragraph::new();
                let mut run = Run::new().highlight("yellow");

                for child in &highlight_node.content {
                    run = self.process_inline_to_run(run, child)?;
                }

                if !run.children.is_empty() {
                    para = para.add_run(run);
                    docx = docx.add_paragraph(para);
                }
            }
            node if node.is_custom_type::<InlineNode>() => {
                let inline_node = node.as_custom_type::<InlineNode>().unwrap();
                // Handle InlineNode at block level (convert to paragraph)
                let mut para = Paragraph::new();
                let mut run = Run::new();

                for child in &inline_node.content {
                    run = self.process_inline_to_run(run, child)?;
                }

                if !run.children.is_empty() {
                    para = para.add_run(run);
                    docx = docx.add_paragraph(para);
                }
            }
            Node::ThematicBreak => {
                // Add horizontal line as specially formatted paragraph
                let hr_para = Paragraph::new()
                    .style("HorizontalLine")
                    .add_run(Run::new().add_text(""));
                docx = docx.add_paragraph(hr_para);
            }
            // Inline elements should not be processed here individually
            _ => {}
        }

        Ok(docx)
    }

    fn process_html_element_block(
        &mut self,
        mut docx: Docx,
        element: &cmark_writer::ast::HtmlElement,
    ) -> Result<Docx> {
        match element.tag.as_str() {
            "figure" => {
                let Some((first, rest)) = element.children.split_first() else {
                    return Ok(docx);
                };
                let synthetic = FigureNode {
                    body: Box::new(first.clone()),
                    caption: rest.to_vec(),
                };
                docx = self.process_figure(docx, &synthetic)?;
            }
            "p" => {
                let is_center = element
                    .attributes
                    .iter()
                    .any(|a| a.name == "align" && a.value == "center");
                if is_center {
                    let start_idx = docx.document.children.len();
                    for child in &element.children {
                        docx = self.process_node(docx, child)?;
                    }
                    for child in docx.document.children.iter_mut().skip(start_idx) {
                        if let DocumentChild::Paragraph(para) = child {
                            para.property = para.property.clone().align(AlignmentType::Center);
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(docx)
    }

    /// Process ordered list
    fn process_ordered_list(&mut self, mut docx: Docx, items: &[ListItem]) -> Result<Docx> {
        // Enter deeper list level
        self.list_level += 1;
        let current_level = self.list_level - 1;

        // Create new ordered list numbering definition
        let (doc, num_id) = self.numbering.create_ordered_numbering(docx);
        docx = doc;

        // Process list items
        for item in items {
            if let ListItem::Ordered { content, .. } = item {
                docx = self.process_list_item_content(docx, content, num_id, current_level)?;
            }
        }

        // Exit list level
        self.list_level -= 1;
        Ok(docx)
    }

    /// Process unordered list
    fn process_unordered_list(&mut self, mut docx: Docx, items: &[ListItem]) -> Result<Docx> {
        // Enter deeper list level
        self.list_level += 1;
        let current_level = self.list_level - 1;

        // Create new unordered list numbering definition
        let (doc, num_id) = self.numbering.create_unordered_numbering(docx);
        docx = doc;

        // Process list items
        for item in items {
            if let ListItem::Unordered { content } = item {
                docx = self.process_list_item_content(docx, content, num_id, current_level)?;
            }
        }

        // Exit list level
        self.list_level -= 1;
        Ok(docx)
    }

    /// Helper function to process list item content
    fn process_list_item_content(
        &mut self,
        mut docx: Docx,
        content: &[Node],
        num_id: usize,
        level: usize,
    ) -> Result<Docx> {
        // If content is empty, add empty paragraph
        if content.is_empty() {
            let empty_para = Paragraph::new()
                .numbering(NumberingId::new(num_id), IndentLevel::new(level))
                .add_run(Run::new().add_text(""));
            return Ok(docx.add_paragraph(empty_para));
        }

        // Process content
        for block in content {
            match block {
                Node::Paragraph(inline) => {
                    let mut para = Paragraph::new()
                        .numbering(NumberingId::new(num_id), IndentLevel::new(level));

                    // Process paragraph content
                    for node in inline {
                        let run = Run::new();
                        let run = self.process_inline_to_run(run, node)?;
                        if !run.children.is_empty() {
                            para = para.add_run(run);
                        }
                    }

                    docx = docx.add_paragraph(para);
                }
                // Recursively process nested lists
                Node::OrderedList { start: _, items: _ } | Node::UnorderedList(_) => {
                    docx = self.process_node(docx, block)?;
                }
                _ => {
                    docx = self.process_node(docx, block)?;
                }
            }
        }

        Ok(docx)
    }

    /// Process table
    fn process_table(
        &self,
        mut docx: Docx,
        columns: usize,
        rows: &[AstTableRow],
        alignments: &[TableAlignment],
    ) -> Result<Docx> {
        if rows.is_empty() || columns == 0 {
            return Ok(docx);
        }

        let mut table = Table::new(vec![]).style("Table");
        let mut vmerge = vec![0usize; columns];

        for row in rows {
            let mut cells = Vec::new();
            let mut col_index = 0;
            let mut cell_iter = row.cells.iter();

            while col_index < columns {
                if vmerge[col_index] > 0 {
                    cells.push(TableCell::new().vertical_merge(VMergeType::Continue));
                    vmerge[col_index] -= 1;
                    col_index += 1;
                    continue;
                }

                if let Some(cell) = cell_iter.next() {
                    let mut effective_align = cell.align.clone();
                    if effective_align.is_none() {
                        if let Some(column_align) = alignments.get(col_index).cloned() {
                            if !matches!(column_align, TableAlignment::None) {
                                effective_align = Some(column_align);
                            }
                        }
                    }

                    let mut table_cell = self.build_table_cell(cell, effective_align)?;
                    if cell.colspan > 1 {
                        table_cell = table_cell.grid_span(cell.colspan);
                    }
                    if cell.rowspan > 1 {
                        table_cell = table_cell.vertical_merge(VMergeType::Restart);
                        for offset in 0..cell.colspan {
                            if col_index + offset < columns {
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

            table = table.add_row(TableRow::new(cells));
        }

        docx = docx.add_table(table);
        Ok(docx)
    }

    fn build_table_cell(
        &self,
        cell: &AstTableCell,
        align: Option<TableAlignment>,
    ) -> Result<TableCell> {
        let mut table_cell = TableCell::new();
        let mut para = Paragraph::new();

        let run = Run::new();
        let run = self.process_inline_to_run(run, &cell.content)?;
        if !run.children.is_empty() {
            para = para.add_run(run);
        }

        if let Some(alignment) = align.and_then(Self::map_table_alignment) {
            para.property = para.property.clone().align(alignment);
        }

        if !para.children.is_empty() {
            table_cell = table_cell.add_paragraph(para);
        }

        Ok(table_cell)
    }

    fn map_table_alignment(alignment: TableAlignment) -> Option<AlignmentType> {
        match alignment {
            TableAlignment::Left => Some(AlignmentType::Left),
            TableAlignment::Center => Some(AlignmentType::Center),
            TableAlignment::Right => Some(AlignmentType::Right),
            TableAlignment::None => None,
        }
    }

    /// Generate DOCX document
    pub fn generate_docx(&mut self, doc: &Node) -> Result<Vec<u8>> {
        // Create DOCX document and initialize styles
        let mut docx = Docx::new();
        docx = self.styles.initialize_styles(docx);

        // Process document content
        docx = self.process_node(docx, doc)?;

        // Initialize numbering definitions
        docx = self.numbering.initialize_numbering(docx);

        // Build and pack document
        let docx_built = docx.build();
        let mut buffer = Vec::new();
        docx_built
            .pack(&mut Cursor::new(&mut buffer))
            .map_err(|e| format!("Failed to pack DOCX: {e}"))?;

        Ok(buffer)
    }
}

impl FormatWriter for DocxWriter {
    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>> {
        self.list_level = 0;
        self.list_numbering_count = 0;
        self.generate_docx(document)
    }

    fn write_eco(&mut self, _document: &Node, _output: &mut EcoString) -> Result<()> {
        Err("DOCX format does not support EcoString output".into())
    }
}

impl IrFormatWriter for DocxWriter {
    fn write_ir_vec(&mut self, document: &ir::Document) -> Result<Vec<u8>> {
        let ast = document.to_cmark_with(ir::CmarkExportTarget::Docx);
        self.write_vec(&ast)
    }

    fn write_ir_eco(&mut self, _document: &ir::Document, _output: &mut EcoString) -> Result<()> {
        Err("DOCX format does not support EcoString output".into())
    }
}
