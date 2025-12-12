//! LaTeX writer implementation

use std::path::Path;

use ecow::EcoString;
use tinymist_std::path::unix_slash;

use crate::Result;
use crate::common::{FormatWriter, ListState};
use crate::ir::{self, Block, Inline, IrNode, ListItem, TableAlignment};

/// LaTeX writer implementation
pub struct LaTeXWriter {
    list_state: Option<ListState>,
}

impl Default for LaTeXWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl LaTeXWriter {
    pub fn new() -> Self {
        Self { list_state: None }
    }

    fn write_document(&mut self, document: &ir::Document, output: &mut EcoString) -> Result<()> {
        for block in &document.blocks {
            self.write_block(block, output)?;
        }
        Ok(())
    }

    fn write_ir_node(&mut self, node: &IrNode, output: &mut EcoString) -> Result<()> {
        match node {
            IrNode::Block(block) => self.write_block(block, output),
            IrNode::Inline(inline) => self.write_inline(inline, output),
        }
    }

    fn write_inline_nodes(&mut self, nodes: &[Inline], output: &mut EcoString) -> Result<()> {
        for node in nodes {
            self.write_inline(node, output)?;
        }
        Ok(())
    }

    fn write_block(&mut self, node: &Block, output: &mut EcoString) -> Result<()> {
        match node {
            Block::Document(blocks) => {
                for block in blocks {
                    self.write_block(block, output)?;
                }
            }
            Block::Paragraph(inlines) => {
                self.write_inline_nodes(inlines, output)?;
                output.push_str("\n\n");
            }
            Block::Heading { level, content } => {
                if *level > 4 {
                    return Err(format!("heading level {level} is not supported in LaTeX").into());
                }

                output.push('\\');
                match level {
                    1 => output.push_str("chapter{"),
                    2 => output.push_str("section{"),
                    3 => output.push_str("subsection{"),
                    4 => output.push_str("subsubsection{"),
                    _ => return Err(format!("Heading level {level} is not supported").into()),
                }

                self.write_inline_nodes(content, output)?;
                output.push_str("}\n\n");
            }
            Block::BlockQuote(content) => {
                output.push_str("\\begin{quote}\n");
                for block in content {
                    self.write_block(block, output)?;
                }
                output.push_str("\\end{quote}\n");
            }
            Block::CodeBlock {
                language,
                content,
                block_type: _,
            } => {
                if let Some(lang) = language {
                    if !lang.is_empty() {
                        output.push_str("\\begin{lstlisting}[language=");
                        output.push_str(lang);
                        output.push_str("]\n");
                    } else {
                        output.push_str("\\begin{verbatim}\n");
                    }
                } else {
                    output.push_str("\\begin{verbatim}\n");
                }

                output.push_str(content);

                if language.as_ref().is_none_or(|lang| lang.is_empty()) {
                    output.push_str("\n\\end{verbatim}");
                } else {
                    output.push_str("\n\\end{lstlisting}");
                }
                output.push_str("\n\n");
            }
            Block::OrderedList { start: _, items } => {
                let previous_state = self.list_state;
                self.list_state = Some(ListState::Ordered);

                output.push_str("\\begin{enumerate}\n");
                for item in items {
                    match item {
                        ListItem::Ordered { content, .. } | ListItem::Unordered { content } => {
                            output.push_str("\\item ");
                            for block in content {
                                match block {
                                    Block::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_block(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
                    }
                }
                output.push_str("\\end{enumerate}\n\n");

                self.list_state = previous_state;
            }
            Block::UnorderedList(items) => {
                let previous_state = self.list_state;
                self.list_state = Some(ListState::Unordered);

                output.push_str("\\begin{itemize}\n");
                for item in items {
                    match item {
                        ListItem::Ordered { content, .. } | ListItem::Unordered { content } => {
                            output.push_str("\\item ");
                            for block in content {
                                match block {
                                    Block::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_block(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
                    }
                }
                output.push_str("\\end{itemize}\n\n");

                self.list_state = previous_state;
            }
            Block::Table(table) => {
                if table.rows.is_empty() {
                    return Ok(());
                }
                let header = &table.rows[0];
                let col_count = header.cells.len();

                output.push_str("\\begin{table}[htbp]\n");
                output.push_str("\\centering\n");
                output.push_str("\\begin{tabular}{");
                for idx in 0..col_count {
                    let spec = match table.alignments.get(idx).unwrap_or(&TableAlignment::Center) {
                        TableAlignment::Left => 'l',
                        TableAlignment::Right => 'r',
                        _ => 'c',
                    };
                    output.push(spec);
                }
                output.push_str("}\n");

                // Write header row
                for (i, cell) in header.cells.iter().enumerate() {
                    if i > 0 {
                        output.push_str(" & ");
                    }
                    for node in &cell.content {
                        self.write_ir_node(node, output)?;
                    }
                }
                output.push_str(" \\\\\n");

                for row in table.rows.iter().skip(1) {
                    for (i, cell) in row.cells.iter().enumerate() {
                        if i > 0 {
                            output.push_str(" & ");
                        }
                        for node in &cell.content {
                            self.write_ir_node(node, output)?;
                        }
                    }
                    output.push_str(" \\\\\n");
                }

                output.push_str("\\end{tabular}\n");
                output.push_str("\\end{table}\n\n");
            }
            Block::Figure { body, caption } => {
                output.push_str("\\begin{figure}[htbp]\n\\centering\n");

                match &**body {
                    Block::Paragraph(inlines) => {
                        for inline in inlines {
                            if let Inline::Image { url, .. } = inline {
                                let path = unix_slash(Path::new(url.as_str()));
                                output.push_str("\\includegraphics[width=0.8\\textwidth]{");
                                output.push_str(&path);
                                output.push_str("}\n");
                            } else {
                                self.write_inline(inline, output)?;
                            }
                        }
                    }
                    other => self.write_block(other, output)?,
                }

                if !caption.is_empty() {
                    output.push_str("\\caption{");
                    self.write_inline_nodes(caption, output)?;
                    output.push_str("}\n");
                }

                output.push_str("\\end{figure}\n\n");
            }
            Block::ExternalFrame(frame) => {
                let path = unix_slash(&frame.file_path);

                output.push_str("\\begin{figure}[htbp]\n");
                output.push_str("\\centering\n");
                output.push_str("\\includegraphics[width=0.8\\textwidth]{");
                output.push_str(&path);
                output.push_str("}\n");

                if !frame.alt_text.is_empty() {
                    output.push_str("\\caption{");
                    output.push_str(&escape_latex(&frame.alt_text));
                    output.push_str("}\n");
                }

                output.push_str("\\end{figure}\n\n");
            }
            Block::Center(inner) => {
                output.push_str("\\begin{center}\n");
                self.write_block(inner, output)?;
                output.push_str("\\end{center}\n\n");
            }
            Block::Alert { class, content } => {
                output.push_str("\\begin{quote}\n");
                output.push_str("\\textbf{[!");
                output.push_str(&escape_latex(class));
                output.push_str("]}\\\\\n");
                for block in content {
                    self.write_block(block, output)?;
                }
                output.push_str("\\end{quote}\n\n");
            }
            Block::ThematicBreak => {
                output.push_str("\\hrule\n\n");
            }
            Block::HtmlElement(element) => {
                if element.tag == "table" {
                    self.write_html_table(element, output)?;
                } else {
                    for child in &element.children {
                        self.write_ir_node(child, output)?;
                    }
                }
            }
            Block::HtmlBlock(_) => {}
        }

        Ok(())
    }

    fn write_inline(&mut self, node: &Inline, output: &mut EcoString) -> Result<()> {
        match node {
            Inline::Text(text) => {
                output.push_str(&escape_latex(text));
            }
            Inline::Emphasis(content) => {
                output.push_str("\\textit{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Inline::Strong(content) => {
                output.push_str("\\textbf{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Inline::Strikethrough(content) => {
                output.push_str("\\sout{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Inline::Group(content) => {
                self.write_inline_nodes(content, output)?;
            }
            Inline::Highlight(content) => {
                output.push_str("\\colorbox{yellow}{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Inline::Link {
                url,
                title: _,
                content,
            } => {
                output.push_str("\\href{");
                output.push_str(url);
                output.push_str("}{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Inline::ReferenceLink { label, content } => {
                if content.is_empty() {
                    output.push_str(&escape_latex(label));
                } else {
                    self.write_inline_nodes(content, output)?;
                }
            }
            Inline::Image { url, alt, .. } => {
                let alt_text = if !alt.is_empty() {
                    let mut alt_str = EcoString::new();
                    self.write_inline_nodes(alt, &mut alt_str)?;
                    alt_str
                } else {
                    "".into()
                };

                let path = unix_slash(Path::new(url.as_str()));

                output.push_str("\\begin{figure}\n");
                output.push_str("\\centering\n");
                output.push_str("\\includegraphics[width=0.8\\textwidth]{");
                output.push_str(&path);
                output.push_str("}\n");

                if !alt_text.is_empty() {
                    output.push_str("\\caption{");
                    output.push_str(&alt_text);
                    output.push_str("}\n");
                }

                output.push_str("\\end{figure}\n\n");
            }
            Inline::InlineCode(code) => {
                output.push_str("\\texttt{");
                output.push_str(&escape_latex(code));
                output.push_str("}");
            }
            Inline::HardBreak => {
                output.push_str("\\\\\n");
            }
            Inline::SoftBreak => {
                output.push(' ');
            }
            Inline::Autolink { url, .. } => {
                output.push_str(url);
            }
            Inline::HtmlElement(element) => {
                if element.tag == "table" {
                    self.write_html_table(element, output)?;
                } else {
                    for child in &element.children {
                        self.write_ir_node(child, output)?;
                    }
                }
            }
            Inline::Verbatim(text) => {
                output.push_str(text);
            }
            Inline::EmbeddedBlock(block) => {
                self.write_block(block, output)?;
            }
            Inline::UnsupportedCustom => {}
        }

        Ok(())
    }

    fn as_html_element<'a>(&self, node: &'a IrNode) -> Option<&'a ir::HtmlElement> {
        match node {
            IrNode::Inline(Inline::HtmlElement(elem)) => Some(elem),
            IrNode::Block(Block::HtmlElement(elem)) => Some(elem),
            _ => None,
        }
    }

    /// Write HTML table element to LaTeX format
    fn write_html_table(
        &mut self,
        table_element: &ir::HtmlElement,
        output: &mut EcoString,
    ) -> Result<()> {
        let mut headers: Vec<Vec<Vec<IrNode>>> = Vec::new();
        let mut rows: Vec<Vec<Vec<IrNode>>> = Vec::new();
        let mut col_count = 0;

        for child in &table_element.children {
            if let Some(elem) = self.as_html_element(child) {
                match elem.tag.as_str() {
                    "thead" => {
                        for row_node in &elem.children {
                            if let Some(row) = self.as_html_element(row_node)
                                && row.tag == "tr"
                            {
                                let cells: Vec<Vec<IrNode>> = row
                                    .children
                                    .iter()
                                    .filter_map(|cell_node| {
                                        if let Some(cell) = self.as_html_element(cell_node)
                                            && (cell.tag == "th" || cell.tag == "td")
                                        {
                                            return Some(cell.children.clone());
                                        }
                                        None
                                    })
                                    .collect();
                                col_count = col_count.max(cells.len());
                                headers.push(cells);
                            }
                        }
                    }
                    "tbody" => {
                        for row_node in &elem.children {
                            if let Some(row) = self.as_html_element(row_node)
                                && row.tag == "tr"
                            {
                                let cells: Vec<Vec<IrNode>> = row
                                    .children
                                    .iter()
                                    .filter_map(|cell_node| {
                                        if let Some(cell) = self.as_html_element(cell_node)
                                            && (cell.tag == "th" || cell.tag == "td")
                                        {
                                            return Some(cell.children.clone());
                                        }
                                        None
                                    })
                                    .collect();
                                col_count = col_count.max(cells.len());
                                rows.push(cells);
                            }
                        }
                    }
                    "tr" => {
                        let cells: Vec<Vec<IrNode>> = elem
                            .children
                            .iter()
                            .filter_map(|cell_node| {
                                if let Some(cell) = self.as_html_element(cell_node)
                                    && (cell.tag == "th" || cell.tag == "td")
                                {
                                    return Some(cell.children.clone());
                                }
                                None
                            })
                            .collect();
                        col_count = col_count.max(cells.len());

                        if headers.is_empty()
                            && elem.children.iter().any(|n| {
                                if let Some(e) = self.as_html_element(n) {
                                    e.tag == "th"
                                } else {
                                    false
                                }
                            })
                        {
                            headers.push(cells);
                        } else {
                            rows.push(cells);
                        }
                    }
                    _ => {}
                }
            }
        }

        if col_count == 0 {
            return Ok(());
        }

        output.push_str("\\begin{table}[htbp]\n");
        output.push_str("\\centering\n");
        output.push_str("\\begin{tabular}{");
        for _ in 0..col_count {
            output.push('c');
        }
        output.push_str("}\n");

        for header_row in &headers {
            for (i, cell_nodes) in header_row.iter().enumerate() {
                if i > 0 {
                    output.push_str(" & ");
                }
                for node in cell_nodes {
                    self.write_ir_node(node, output)?;
                }
            }
            output.push_str(" \\\\\n");
        }

        for row in &rows {
            for (i, cell_nodes) in row.iter().enumerate() {
                if i > 0 {
                    output.push_str(" & ");
                }
                for node in cell_nodes {
                    self.write_ir_node(node, output)?;
                }
            }
            output.push_str(" \\\\\n");
        }

        output.push_str("\\end{tabular}\n");
        output.push_str("\\end{table}\n\n");

        Ok(())
    }
}

impl FormatWriter for LaTeXWriter {
    fn write_eco(
        &mut self,
        document: &cmark_writer::ast::Node,
        output: &mut EcoString,
    ) -> Result<()> {
        let ir_doc = ir::Document::from_cmark(document);
        self.write_document(&ir_doc, output)
    }

    fn write_vec(&mut self, document: &cmark_writer::ast::Node) -> Result<Vec<u8>> {
        let mut output = EcoString::new();
        self.write_eco(document, &mut output)?;
        Ok(output.as_str().as_bytes().to_vec())
    }
}

/// Escape LaTeX special characters in a string
fn escape_latex(text: &str) -> String {
    text.replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
        .replace('\\', "\\textbackslash{}")
}
