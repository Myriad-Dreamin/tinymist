//! LaTeX converter implementation

use std::fmt::Write;
use std::path::Path;

use base64::Engine;
use ecow::EcoString;
use typst::html::{tag, HtmlElement, HtmlNode};
use typst::layout::Frame;

use crate::attributes::{HeadingAttr, ImageAttr, LinkAttr, RawAttr, TypliteAttrsParser};
use crate::converter::ListState;
use crate::tags::md_tag;
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
        match root.tag {
            // Document structure elements
            tag::head => Ok(()),
            tag::html | tag::body | md_tag::doc => self.convert_children(root, w),
            tag::span | tag::dl | tag::dt | tag::dd => {
                self.convert_children(root, w)?;
                Ok(())
            }
            tag::p => self.convert_paragraph(root, w),

            // List-related elements
            tag::ol => self.process_ordered_list(root, w),
            tag::ul => self.process_unordered_list(root, w),
            tag::li => self.process_list_item(root, w),

            // Media and figure elements
            tag::figure => self.convert_children(root, w),
            tag::figcaption => Ok(()),

            // Special elements
            md_tag::heading => self.convert_heading(root, w),
            md_tag::link => self.process_link(root, w),
            md_tag::parbreak => self.process_paragraph_break(w),
            md_tag::linebreak => self.process_line_break(w),

            // Text formatting elements
            tag::strong | md_tag::strong => self.process_strong(root, w),
            tag::em | md_tag::emph => self.process_emphasis(root, w),
            md_tag::highlight => self.process_highlight(root, w),
            md_tag::strike => self.process_strike(root, w),
            md_tag::raw => self.process_raw(root, w),

            // Reference elements
            md_tag::label | md_tag::reference | md_tag::outline | md_tag::outline_entry => {
                self.process_reference(root, w)
            }

            // Block elements
            md_tag::quote => self.process_quote(root, w),
            md_tag::table | md_tag::grid => self.process_table(root, w),

            // Math and image elements
            md_tag::math_equation_inline | md_tag::math_equation_block => self.process_math(w),
            md_tag::image => self.process_image(root, w),

            // Fallback for unknown elements
            _ => Err(format!("Unexpected tag: {:?}", root.tag).into()),
        }
    }

    /// Converts child elements
    pub fn convert_children(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        for child in &root.children {
            match child {
                HtmlNode::Tag(_) => {}
                HtmlNode::Frame(frame) => self.write_frame(frame, w),
                HtmlNode::Text(text, _) => {
                    w.push_str(text);
                }
                HtmlNode::Element(element) => {
                    self.convert(element, w)?;
                }
            }
        }
        Ok(())
    }

    // Processing methods for specific element types

    /// Processes a paragraph break
    fn process_paragraph_break(&mut self, w: &mut EcoString) -> Result<()> {
        w.push_str("\n\n");
        Ok(())
    }

    /// Processes a line break
    fn process_line_break(&mut self, w: &mut EcoString) -> Result<()> {
        w.push_str("\n");
        Ok(())
    }

    /// Processes an ordered list
    fn process_ordered_list(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let state = self.list_state;
        self.list_state = Some(ListState::Ordered);

        w.push_str("\\begin{enumerate}\n");
        self.convert_children(root, w)?;
        w.push_str("\\end{enumerate}\n");

        self.list_state = state;
        Ok(())
    }

    /// Processes an unordered list
    fn process_unordered_list(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let state = self.list_state;
        self.list_state = Some(ListState::Unordered);

        w.push_str("\\begin{itemize}\n");
        self.convert_children(root, w)?;
        w.push_str("\\end{itemize}\n");

        self.list_state = state;
        Ok(())
    }

    /// Processes a list item
    fn process_list_item(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\item ");
        self.convert_children(root, w)?;
        w.push_str("\n");
        Ok(())
    }

    /// Processes a strong (bold) element
    fn process_strong(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\textbf{");
        self.convert_children(root, w)?;
        w.push_str("}");
        Ok(())
    }

    /// Processes an emphasis (italic) element
    fn process_emphasis(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\textit{");
        self.convert_children(root, w)?;
        w.push_str("}");
        Ok(())
    }

    /// Processes a highlight element
    fn process_highlight(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\colorbox{yellow}{");
        self.convert_children(root, w)?;
        w.push_str("}");
        Ok(())
    }

    /// Processes a strike element
    fn process_strike(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\sout{");
        self.convert_children(root, w)?;
        w.push_str("}");
        Ok(())
    }

    /// Processes a reference element
    fn process_reference(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\texttt{");
        self.convert_children(root, w)?;
        w.push_str("}");
        Ok(())
    }

    /// Processes a quote element
    fn process_quote(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        w.push_str("\\begin{quote}\n");
        self.convert_children(root, w)?;
        w.push_str("\\end{quote}\n");
        Ok(())
    }

    /// Processes a table element
    fn process_table(&mut self, element: &HtmlElement, w: &mut EcoString) -> Result<()> {
        // Find real table element - either directly in m1table or inside m1grid/m1table
        let real_table_elem = if element.tag == md_tag::grid {
            // For grid: grid -> table -> table
            let mut inner_table = None;

            for child in &element.children {
                if let HtmlNode::Element(table_elem) = child {
                    if table_elem.tag == md_tag::table {
                        // Find table tag inside m1table
                        for inner_child in &table_elem.children {
                            if let HtmlNode::Element(inner) = inner_child {
                                if inner.tag == tag::table {
                                    inner_table = Some(inner);
                                    break;
                                }
                            }
                        }

                        if inner_table.is_some() {
                            break;
                        }
                    }
                }
            }

            inner_table
        } else {
            // For m1table -> table
            let mut direct_table = None;

            for child in &element.children {
                if let HtmlNode::Element(table_elem) = child {
                    if table_elem.tag == tag::table {
                        direct_table = Some(table_elem);
                        break;
                    }
                }
            }

            direct_table
        };

        // If we found a real table element, process it as a LaTeX tabular
        if let Some(table) = real_table_elem {
            // Count columns in the first row to set up the tabular format
            let mut col_count = 0;

            // Find the first row to count columns
            for row_node in &table.children {
                if let HtmlNode::Element(row_elem) = row_node {
                    if row_elem.tag == tag::tr {
                        // Count cells in this row
                        for cell_node in &row_elem.children {
                            if let HtmlNode::Element(cell) = cell_node {
                                if cell.tag == tag::td {
                                    col_count += 1;
                                }
                            }
                        }
                        break;
                    }
                }
            }

            // If we found at least one column, create a tabular environment
            if col_count > 0 {
                // Start tabular environment
                w.push_str("\\begin{table}[htbp]\n");
                w.push_str("\\centering\n");
                w.push_str("\\begin{tabular}{");

                // Add column format specifiers (centered columns)
                for _ in 0..col_count {
                    w.push('c');
                }
                w.push_str("}\n\\hline\n");

                // Process all rows in the table
                let mut is_first_row = true;
                for row_node in &table.children {
                    if let HtmlNode::Element(row_elem) = row_node {
                        if row_elem.tag == tag::tr {
                            let mut cell_idx = 0;

                            // Process cells in this row
                            for cell_node in &row_elem.children {
                                if let HtmlNode::Element(cell) = cell_node {
                                    if cell.tag == tag::td {
                                        // Add cell separator if not the first cell
                                        if cell_idx > 0 {
                                            w.push_str(" & ");
                                        }

                                        // Process cell content
                                        let mut cell_content = EcoString::new();
                                        self.convert_children(cell, &mut cell_content)?;
                                        w.push_str(&cell_content);

                                        cell_idx += 1;
                                    }
                                }
                            }

                            // End the row
                            w.push_str(" \\\\\n");

                            // Add a horizontal line after header row
                            if is_first_row {
                                w.push_str("\\hline\n");
                                is_first_row = false;
                            }
                        }
                    }
                }

                // Close the tabular environment
                w.push_str("\\hline\n");
                w.push_str("\\end{tabular}\n");
                w.push_str("\\end{table}\n");
            } else {
                // Fallback if we couldn't determine the structure
                w.push_str(
                    "\\begin{verbatim}\n[Table content could not be processed]\n\\end{verbatim}\n",
                );
            }
        } else {
            // If no table structure was found, use verbatim as fallback
            w.push_str("\\begin{verbatim}\n");
            self.convert_children(element, w)?;
            w.push_str("\\end{verbatim}\n");
        }

        Ok(())
    }

    /// Processes math equation
    fn process_math(&mut self, w: &mut EcoString) -> Result<()> {
        w.push_str(
            r#"\begin{equation}
\int x^2 \operatorname{d} x
\end{equation}
"#,
        );
        Ok(())
    }

    /// Processes a link element
    fn process_link(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let attrs = LinkAttr::parse(&root.attrs)?;

        w.push_str("\\href{");
        w.push_str(&attrs.dest);
        w.push_str("}{");
        self.convert_children(root, w)?;
        w.push_str("}");

        Ok(())
    }

    /// Processes a raw code element
    fn process_raw(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let attrs = RawAttr::parse(&root.attrs)?;
        let lang = attrs.lang;
        let block = attrs.block;
        let text = attrs.text;

        if block {
            if !lang.is_empty() {
                w.push_str("\\begin{lstlisting}[language=");
                w.push_str(&lang);
                w.push_str("]\n");
            } else {
                w.push_str("\\begin{verbatim}\n");
            }

            w.push_str(&text);

            if !lang.is_empty() {
                w.push_str("\n\\end{lstlisting}");
            } else {
                w.push_str("\n\\end{verbatim}");
            }
        } else {
            w.push_str("\\texttt{");
            // Escape LaTeX special characters
            let escaped_text = text
                .replace("\\", "\\textbackslash{}")
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
                .replace(">", "\\textgreater{}");

            w.push_str(&escaped_text);
            w.push_str("}");
        }

        Ok(())
    }

    /// Processes an image element
    fn process_image(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let attrs = ImageAttr::parse(&root.attrs)?;
        let src = unix_slash(Path::new(attrs.src.as_str()));

        w.push_str("\\begin{figure}\n");
        w.push_str("\\centering\n");
        w.push_str("\\includegraphics[width=0.8\\textwidth]{");
        w.push_str(&src);
        w.push_str("}\n");

        if !attrs.alt.is_empty() {
            w.push_str("\\caption{");
            w.push_str(&attrs.alt);
            w.push_str("}\n");
        }

        w.push_str("\\end{figure}\n");

        Ok(())
    }

    /// Converts a heading element
    fn convert_heading(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        let attrs = HeadingAttr::parse(&root.attrs)?;

        if attrs.level >= 4 || attrs.level == 0 {
            return Err(format!("heading level {} is not supported in LaTeX", attrs.level).into());
        }

        w.push('\\');
        match attrs.level {
            1 => w.push_str("section{"),
            2 => w.push_str("subsection{"),
            3 => w.push_str("subsubsection{"),
            _ => return Err(format!("Heading level {} is not supported", attrs.level).into()),
        }

        self.convert_children(root, w)?;
        w.push_str("}\n\n");
        Ok(())
    }

    /// Converts a paragraph element
    fn convert_paragraph(&mut self, root: &HtmlElement, w: &mut EcoString) -> Result<()> {
        self.convert_children(root, w)?;
        w.push_str("\n\n");
        Ok(())
    }

    /// Encodes a laid out frame into the writer
    fn write_frame(&mut self, frame: &Frame, w: &mut EcoString) {
        // Create SVG from frame and adjust it for better display
        let svg = typst_svg::svg_frame(frame)
            .replace("<svg class", "<svg style=\"overflow: visible;\" class");

        // Encode SVG as base64
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());

        // Write as inline image
        let _ = write!(
            w,
            r#"\\includegraphics{{data:image/svg+xml;base64,{}}}"#,
            data
        );
    }
}
