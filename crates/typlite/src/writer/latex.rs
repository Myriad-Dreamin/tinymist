//! LaTeX writer implementation

use std::path::Path;

use cmark_writer::ast::{Node, TableAlignment};
use ecow::EcoString;
use tinymist_std::path::unix_slash;

use crate::Result;
use crate::common::{
    CenterNode, ExternalFrameNode, FigureNode, FormatWriter, HighlightNode, InlineNode, ListState,
    VerbatimNode,
};

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

    fn write_inline_nodes(&mut self, nodes: &[Node], output: &mut EcoString) -> Result<()> {
        for node in nodes {
            self.write_node(node, output)?;
        }
        Ok(())
    }

    /// Write the document to LaTeX format
    fn write_node(&mut self, node: &Node, output: &mut EcoString) -> Result<()> {
        match node {
            Node::Document(blocks) => {
                for block in blocks {
                    self.write_node(block, output)?;
                }
            }
            Node::Paragraph(inlines) => {
                self.write_inline_nodes(inlines, output)?;
                output.push_str("\n\n");
            }
            Node::Heading {
                level,
                content,
                heading_type: _,
            } => {
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
            Node::BlockQuote(content) => {
                output.push_str("\\begin{quote}\n");
                for block in content {
                    self.write_node(block, output)?;
                }
                output.push_str("\\end{quote}\n");
            }
            Node::CodeBlock {
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
            Node::OrderedList { start: _, items } => {
                let previous_state = self.list_state;
                self.list_state = Some(ListState::Ordered);

                output.push_str("\\begin{enumerate}\n");
                for item in items {
                    match item {
                        cmark_writer::ast::ListItem::Ordered { content, .. }
                        | cmark_writer::ast::ListItem::Unordered { content } => {
                            output.push_str("\\item ");
                            for block in content {
                                match block {
                                    // For paragraphs, we want inline content rather than creating a
                                    // new paragraph
                                    Node::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_node(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
                        _ => {}
                    }
                }
                output.push_str("\\end{enumerate}\n\n");

                self.list_state = previous_state;
            }
            Node::UnorderedList(items) => {
                let previous_state = self.list_state;
                self.list_state = Some(ListState::Unordered);

                output.push_str("\\begin{itemize}\n");
                for item in items {
                    match item {
                        cmark_writer::ast::ListItem::Ordered { content, .. }
                        | cmark_writer::ast::ListItem::Unordered { content } => {
                            output.push_str("\\item ");
                            for block in content {
                                match block {
                                    // For paragraphs, we want inline content rather than creating a
                                    // new paragraph
                                    Node::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_node(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
                        _ => {}
                    }
                }
                output.push_str("\\end{itemize}\n\n");

                self.list_state = previous_state;
            }
            Node::Table {
                rows, alignments, ..
            } => {
                if rows.is_empty() {
                    return Ok(());
                }
                let header = &rows[0];
                let col_count = header.cells.len();

                output.push_str("\\begin{table}[htbp]\n");
                output.push_str("\\centering\n");
                output.push_str("\\begin{tabular}{");
                for idx in 0..col_count {
                    let spec = match alignments.get(idx).unwrap_or(&TableAlignment::Center) {
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
                    self.write_node(&cell.content, output)?;
                }
                output.push_str(" \\\\\n");

                for row in rows.iter().skip(1) {
                    for (i, cell) in row.cells.iter().enumerate() {
                        if i > 0 {
                            output.push_str(" & ");
                        }
                        self.write_node(&cell.content, output)?;
                    }
                    output.push_str(" \\\\\n");
                }

                output.push_str("\\end{tabular}\n");
                output.push_str("\\end{table}\n\n");
            }
            node if node.is_custom_type::<FigureNode>() => {
                let figure_node = node.as_custom_type::<FigureNode>().unwrap();
                // Start figure environment
                output.push_str("\\begin{figure}[htbp]\n\\centering\n");

                // Handle the body content (typically an image)
                match &*figure_node.body {
                    Node::Paragraph(content) => {
                        for node in content {
                            // Special handling for image nodes in figures
                            if let Node::Image {
                                url,
                                title: _,
                                alt: _,
                            } = node
                            {
                                // Path to the image file
                                let path = unix_slash(Path::new(url.as_str()));

                                // Write includegraphics command
                                output.push_str("\\includegraphics[width=0.8\\textwidth]{");
                                output.push_str(&path);
                                output.push_str("}\n");
                            } else {
                                // For non-image content, just render it normally
                                self.write_node(node, output)?;
                            }
                        }
                    }
                    // Directly handle the node if it's not in a paragraph
                    node => self.write_node(node, output)?,
                }

                // Add caption if present
                if !figure_node.caption.is_empty() {
                    output.push_str("\\caption{");
                    self.write_inline_nodes(&figure_node.caption, output)?;
                    output.push_str("}\n");
                }

                // Close figure environment
                output.push_str("\\end{figure}\n\n");
            }
            node if node.is_custom_type::<ExternalFrameNode>() => {
                let external_frame = node.as_custom_type::<ExternalFrameNode>().unwrap();
                // Handle externally stored frames
                let path = unix_slash(&external_frame.file_path);

                output.push_str("\\begin{figure}[htbp]\n");
                output.push_str("\\centering\n");
                output.push_str("\\includegraphics[width=0.8\\textwidth]{");
                output.push_str(&path);
                output.push_str("}\n");

                if !external_frame.alt_text.is_empty() {
                    output.push_str("\\caption{");
                    output.push_str(&escape_latex(&external_frame.alt_text));
                    output.push_str("}\n");
                }

                output.push_str("\\end{figure}\n\n");
            }
            node if node.is_custom_type::<CenterNode>() => {
                let center_node = node.as_custom_type::<CenterNode>().unwrap();
                output.push_str("\\begin{center}\n");
                self.write_node(&center_node.node, output)?;
                output.push_str("\\end{center}\n\n");
            }
            node if node.is_custom_type::<HighlightNode>() => {
                let highlight_node = node.as_custom_type::<HighlightNode>().unwrap();
                output.push_str("\\colorbox{yellow}{");
                for child in &highlight_node.content {
                    self.write_node(child, output)?;
                }
                output.push_str("}");
            }
            node if node.is_custom_type::<InlineNode>() => {
                let inline_node = node.as_custom_type::<InlineNode>().unwrap();
                // Process all child nodes inline
                for child in &inline_node.content {
                    self.write_node(child, output)?;
                }
            }
            node if node.is_custom_type::<VerbatimNode>() => {
                let inline_node = node.as_custom_type::<VerbatimNode>().unwrap();
                output.push_str(&inline_node.content);
            }
            Node::Text(text) => {
                output.push_str(&escape_latex(text));
            }
            Node::Emphasis(content) => {
                output.push_str("\\textit{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Node::Strong(content) => {
                output.push_str("\\textbf{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Node::Strikethrough(content) => {
                output.push_str("\\sout{");
                self.write_inline_nodes(content, output)?;
                output.push_str("}");
            }
            Node::Link {
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
            Node::Image { url, title: _, alt } => {
                let alt_text = if !alt.is_empty() {
                    let mut alt_str = EcoString::new();
                    self.write_inline_nodes(alt, &mut alt_str)?;
                    alt_str
                } else {
                    "".into()
                };

                let path = unix_slash(Path::new(&url.as_str()));

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
            Node::InlineCode(code) => {
                output.push_str("\\texttt{");
                output.push_str(&escape_latex(code));
                output.push_str("}");
            }
            Node::HardBreak => {
                output.push_str("\\\\\n");
            }
            Node::SoftBreak => {
                output.push(' ');
            }
            Node::ThematicBreak => {
                output.push_str("\\hrule\n\n");
            }
            Node::HtmlElement(element) => {
                if element.tag == "table" {
                    self.write_html_table(element, output)?;
                } else {
                    for child in &element.children {
                        self.write_node(child, output)?;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Write HTML table element to LaTeX format
    fn write_html_table(
        &mut self,
        table_element: &cmark_writer::ast::HtmlElement,
        output: &mut EcoString,
    ) -> Result<()> {
        // Collect rows and determine column count
        let mut headers: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut rows: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut col_count = 0;

        // Process table structure
        for child in &table_element.children {
            if let Node::HtmlElement(elem) = child {
                match elem.tag.as_str() {
                    "thead" => {
                        for row_node in &elem.children {
                            if let Node::HtmlElement(row) = row_node {
                                if row.tag == "tr" {
                                    let cells: Vec<Vec<Node>> = row
                                        .children
                                        .iter()
                                        .filter_map(|cell_node| {
                                            if let Node::HtmlElement(cell) = cell_node {
                                                if cell.tag == "th" || cell.tag == "td" {
                                                    return Some(cell.children.clone());
                                                }
                                            }
                                            None
                                        })
                                        .collect();
                                    col_count = col_count.max(cells.len());
                                    headers.push(cells);
                                }
                            }
                        }
                    }
                    "tbody" => {
                        for row_node in &elem.children {
                            if let Node::HtmlElement(row) = row_node {
                                if row.tag == "tr" {
                                    let cells: Vec<Vec<Node>> = row
                                        .children
                                        .iter()
                                        .filter_map(|cell_node| {
                                            if let Node::HtmlElement(cell) = cell_node {
                                                if cell.tag == "th" || cell.tag == "td" {
                                                    return Some(cell.children.clone());
                                                }
                                            }
                                            None
                                        })
                                        .collect();
                                    col_count = col_count.max(cells.len());
                                    rows.push(cells);
                                }
                            }
                        }
                    }
                    "tr" => {
                        // Direct row without thead/tbody
                        let cells: Vec<Vec<Node>> = elem
                            .children
                            .iter()
                            .filter_map(|cell_node| {
                                if let Node::HtmlElement(cell) = cell_node {
                                    if cell.tag == "th" || cell.tag == "td" {
                                        return Some(cell.children.clone());
                                    }
                                }
                                None
                            })
                            .collect();
                        col_count = col_count.max(cells.len());

                        // First row with th elements is header
                        if headers.is_empty()
                            && elem.children.iter().any(|n| {
                                if let Node::HtmlElement(e) = n {
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

        // Write LaTeX table
        output.push_str("\\begin{table}[htbp]\n");
        output.push_str("\\centering\n");
        output.push_str("\\begin{tabular}{");
        for _ in 0..col_count {
            output.push('c');
        }
        output.push_str("}\n");

        // Write headers
        for header_row in &headers {
            for (i, cell_nodes) in header_row.iter().enumerate() {
                if i > 0 {
                    output.push_str(" & ");
                }
                self.write_inline_nodes(cell_nodes, output)?;
            }
            output.push_str(" \\\\\n");
        }

        // Write rows
        for row in &rows {
            for (i, cell_nodes) in row.iter().enumerate() {
                if i > 0 {
                    output.push_str(" & ");
                }
                self.write_inline_nodes(cell_nodes, output)?;
            }
            output.push_str(" \\\\\n");
        }

        output.push_str("\\end{tabular}\n");
        output.push_str("\\end{table}\n\n");

        Ok(())
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

impl FormatWriter for LaTeXWriter {
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()> {
        // Write the document content
        self.write_node(document, output)?;
        Ok(())
    }

    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>> {
        let mut output = EcoString::new();
        self.write_eco(document, &mut output)?;
        Ok(output.as_str().as_bytes().to_vec())
    }
}
