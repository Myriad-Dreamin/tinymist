//! DOCX document writer implementation

use cmark_writer::ast::{ListItem, Node};
use docx_rs::*;
use ecow::EcoString;
use std::fs;
use std::io::Cursor;

use crate::converter::FormatWriter;
use crate::Result;
use crate::TypliteFeat;

use super::image_processor::DocxImageProcessor;
use super::numbering::DocxNumbering;
use super::styles::DocxStyles;

/// DOCX writer that generates DOCX directly from AST (without intermediate representation)
pub struct DocxWriter {
    _feat: TypliteFeat,
    styles: DocxStyles,
    numbering: DocxNumbering,
    list_level: usize,
    list_numbering_count: usize,
    image_processor: DocxImageProcessor,
}

impl DocxWriter {
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            _feat: feat,
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
            Ok(self.image_processor.process_image_data(docx, &img_data, alt_text.as_deref(), None))
        } else {
            let placeholder = format!("[Image not found: {}]", url);
            let para = Paragraph::new().add_run(Run::new().add_text(placeholder));
            Ok(docx.add_paragraph(para))
        }
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
            Node::Strike(content) => {
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
                if let Ok(img_data) = fs::read(url) {
                    run = self.image_processor.process_inline_image(run, &img_data)?;
                } else {
                    run = run.add_text(format!("[Image not found: {}]", url));
                }
            }
            Node::HtmlElement(element) => {
                // Handle special HTML elements
                if element.tag == "mark" {
                    run = run.style("Highlight");
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
                        run = self.image_processor.process_data_url_image(run, src, is_typst_block)?;
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
            // Other inline element types
            _ => {}
        }

        Ok(run)
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
            Node::Heading { level, content } => {
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
            Node::CodeBlock { language, content } => {
                // Add language information
                if let Some(lang) = language {
                    if !lang.is_empty() {
                        let lang_para = Paragraph::new()
                            .style("CodeBlock")
                            .add_run(Run::new().add_text(lang));
                        docx = docx.add_paragraph(lang_para);
                    }
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
                headers,
                rows,
                alignments: _,
            } => {
                docx = self.process_table(docx, headers, rows)?;
            }
            Node::Image { url, title: _, alt } => {
                docx = self.process_image(docx, url, alt)?;
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
                for block in content {
                    match block {
                        Node::Paragraph(inline) => {
                            let mut para = Paragraph::new().numbering(
                                NumberingId::new(num_id),
                                IndentLevel::new(current_level),
                            );

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

                // If list item content is empty, add empty paragraph
                if content.is_empty() {
                    let empty_para = Paragraph::new()
                        .numbering(NumberingId::new(num_id), IndentLevel::new(current_level))
                        .add_run(Run::new().add_text(""));
                    docx = docx.add_paragraph(empty_para);
                }
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
                for block in content {
                    match block {
                        Node::Paragraph(inline) => {
                            let mut para = Paragraph::new().numbering(
                                NumberingId::new(num_id),
                                IndentLevel::new(current_level),
                            );

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

                // If list item content is empty, add empty paragraph
                if content.is_empty() {
                    let empty_para = Paragraph::new()
                        .numbering(NumberingId::new(num_id), IndentLevel::new(current_level))
                        .add_run(Run::new().add_text(""));
                    docx = docx.add_paragraph(empty_para);
                }
            }
        }

        // Exit list level
        self.list_level -= 1;
        Ok(docx)
    }

    /// Process table
    fn process_table(&self, mut docx: Docx, headers: &[Node], rows: &[Vec<Node>]) -> Result<Docx> {
        let mut table = Table::new(vec![]).style("Table");

        // Process table headers
        if !headers.is_empty() {
            let mut cells = Vec::new();

            for header_node in headers {
                let mut table_cell = TableCell::new();
                let mut para = Paragraph::new();

                let run = Run::new();
                let run = self.process_inline_to_run(run, header_node)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }

                if !para.children.is_empty() {
                    table_cell = table_cell.add_paragraph(para);
                }

                cells.push(table_cell);
            }

            if !cells.is_empty() {
                let header_row = TableRow::new(cells);
                table = table.add_row(header_row);
            }
        }

        // Process table rows
        for row in rows {
            let mut cells = Vec::new();

            for cell_node in row {
                let mut table_cell = TableCell::new();
                let mut para = Paragraph::new();

                let run = Run::new();
                let run = self.process_inline_to_run(run, cell_node)?;
                if !run.children.is_empty() {
                    para = para.add_run(run);
                }

                if !para.children.is_empty() {
                    table_cell = table_cell.add_paragraph(para);
                }

                cells.push(table_cell);
            }

            if !cells.is_empty() {
                let data_row = TableRow::new(cells);
                table = table.add_row(data_row);
            }
        }

        // Add table to document
        docx = docx.add_table(table);
        
        Ok(docx)
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
            .map_err(|e| format!("Failed to pack DOCX: {}", e))?;

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