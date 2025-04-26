//! LaTeX converter implementation

use std::path::Path;

use cmark_writer::ast::Node;
use ecow::EcoString;
use typst::html::HtmlElement;

use crate::converter::{FormatWriter, HtmlToAstParser, ListState};
use crate::tinymist_std::path::unix_slash;
use crate::Result;
use crate::TypliteFeat;

/// LaTeX converter implementation
#[derive(Clone)]
pub struct LaTeXConverter {
    pub feat: TypliteFeat,
    pub list_state: Option<ListState>,
}

impl LaTeXConverter {
    /// Creates a new LaTeXConverter instance
    pub fn new(feat: TypliteFeat) -> Self {
        Self {
            feat,
            list_state: None,
        }
    }

    /// Converts an HTML element to LaTeX format
    pub fn convert(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        // Parse HTML to AST using shared parser
        let parser = HtmlToAstParser::new(self.feat.clone());
        let document = parser.parse(root)?;

        // Convert AST to LaTeX using LaTeX writer
        let mut writer = LaTeXWriter::new();
        writer.write_eco(&document, w)?;

        Ok(())
    }
}

/// LaTeX writer implementation
#[derive(Default)]
pub struct LaTeXWriter {
    list_state: Option<ListState>,
}

impl LaTeXWriter {
    pub fn new() -> Self {
        Self { list_state: None }
    }

    /// Generate LaTeX document preamble with necessary package imports
    fn write_preamble(&self, output: &mut EcoString) {
        output.push_str("\\documentclass[12pt,a4paper]{article}\n");
        output.push_str("\\usepackage[utf8]{inputenc}\n");
        output.push_str("\\usepackage{hyperref}\n"); // For links
        output.push_str("\\usepackage{graphicx}\n"); // For images
        output.push_str("\\usepackage{ulem}\n"); // For strikethrough \sout
        output.push_str("\\usepackage{listings}\n"); // For code blocks
        output.push_str("\\usepackage{xcolor}\n"); // For colored text and backgrounds
        output.push_str("\\usepackage{amsmath}\n"); // Math formula support
        output.push_str("\\usepackage{amssymb}\n"); // Additional math symbols
        output.push_str("\\usepackage{array}\n"); // Enhanced table functionality

        // Set code highlighting style
        output.push_str("\\lstset{\n");
        output.push_str("  basicstyle=\\ttfamily\\small,\n");
        output.push_str("  breaklines=true,\n");
        output.push_str("  frame=single,\n");
        output.push_str("  numbers=left,\n");
        output.push_str("  numberstyle=\\tiny,\n");
        output.push_str("  keywordstyle=\\color{blue},\n");
        output.push_str("  commentstyle=\\color{green!60!black},\n");
        output.push_str("  stringstyle=\\color{red}\n");
        output.push_str("}\n\n");

        output.push_str("\\begin{document}\n\n");
    }

    fn write_inline_nodes(&mut self, nodes: &[Node], output: &mut EcoString) -> Result<()> {
        for node in nodes {
            self.write_node(node, output)?;
        }
        Ok(())
    }

    fn escape_latex(&self, text: &str) -> String {
        text.replace("\\", "\\textbackslash{}")
            .replace("{", "\\{")
            .replace("}", "\\}")
            .replace("_", "\\_")
            .replace("^", "\\^")
            .replace("&", "\\&")
            .replace("%", "\\%")
            .replace("$", "\\$")
            .replace("#", "\\#")
            .replace("~", "\\~{}")
            .replace("<", "\\textless{}")
            .replace(">", "\\textgreater{}")
    }

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
            Node::Heading { level, content } => {
                if *level > 4 {
                    return Err(format!("heading level {} is not supported in LaTeX", level).into());
                }

                output.push('\\');
                match level {
                    1 => output.push_str("chapter{"),
                    2 => output.push_str("section{"),
                    3 => output.push_str("subsection{"),
                    4 => output.push_str("subsubsection{"),
                    _ => return Err(format!("Heading level {} is not supported", level).into()),
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
            Node::CodeBlock { language, content } => {
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
                                    // For paragraphs, we want inline content rather than creating a new paragraph
                                    Node::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_node(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
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
                                    // For paragraphs, we want inline content rather than creating a new paragraph
                                    Node::Paragraph(inlines) => {
                                        self.write_inline_nodes(inlines, output)?;
                                    }
                                    _ => self.write_node(block, output)?,
                                }
                            }
                            output.push('\n');
                        }
                    }
                }
                output.push_str("\\end{itemize}\n\n");

                self.list_state = previous_state;
            }
            Node::Table {
                headers,
                rows,
                alignments: _,
            } => {
                // Calculate column count
                let col_count = headers
                    .len()
                    .max(rows.iter().map(|row| row.len()).max().unwrap_or(0));

                output.push_str("\\begin{table}[htbp]\n");
                output.push_str("\\centering\n");
                output.push_str("\\begin{tabular}{");

                // Add column format (centered alignment)
                for _ in 0..col_count {
                    output.push('c');
                }
                output.push_str("}\n\\hline\n");

                // Process header
                if !headers.is_empty() {
                    for (i, cell) in headers.iter().enumerate() {
                        if i > 0 {
                            output.push_str(" & ");
                        }
                        self.write_node(cell, output)?;
                    }
                    output.push_str(" \\\\\n\\hline\n");
                }

                // Process all rows
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            output.push_str(" & ");
                        }
                        self.write_node(cell, output)?;
                    }
                    output.push_str(" \\\\\n");
                }

                // Close table environment
                output.push_str("\\hline\n");
                output.push_str("\\end{tabular}\n");
                output.push_str("\\end{table}\n\n");
            }
            Node::Text(text) => {
                output.push_str(&self.escape_latex(text));
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
            Node::Strike(content) => {
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

                let path = unix_slash(Path::new(url));

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
                output.push_str(&self.escape_latex(code));
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
                if element.tag == "mark" {
                    output.push_str("\\colorbox{yellow}{");
                    for child in &element.children {
                        self.write_node(child, output)?;
                    }
                    output.push_str("}");
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
}

impl FormatWriter for LaTeXWriter {
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()> {
        // Write LaTeX preamble with necessary package imports
        self.write_preamble(output);

        // Write document main content
        self.write_node(document, output)?;

        // Add document end tag
        output.push_str("\n\\end{document}");

        Ok(())
    }

    fn write_vec(&mut self, _document: &Node) -> Result<Vec<u8>> {
        Err("LaTeXWriter does not support write_vec.".to_string().into())
    }
}
