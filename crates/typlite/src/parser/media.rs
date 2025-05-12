//! Media processing module, handles images, SVG and Frame media elements

use std::path::Path;

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use typst::{
    foundations::Content,
    introspection::Introspector,
    layout::{Frame, Page, PagedDocument},
    model::DocumentInfo,
};

use crate::common::{ExternalFrameNode, Format};

use super::core::HtmlToAstParser;

impl HtmlToAstParser {
    /// Convert Typst frame to CommonMark node
    pub fn convert_frame(&mut self, frame: &Frame) -> Node {
        match self.feat.target {
            Format::LaTeX => self.convert_pdf_frame(frame),
            _ => self.convert_svg_frame(frame),
        }
    }

    /// Convert Pdf frame to CommonMark node
    pub fn convert_pdf_frame(&mut self, frame: &Frame) -> Node {
        let page = Page {
            frame: frame.clone(),
            fill: typst::foundations::Smart::Custom(None),
            numbering: None,
            supplement: Content::default(),
            number: 1,
        };

        let pages = vec![page];
        let introspector = Introspector::paged(&pages);

        let doc = PagedDocument {
            pages,
            info: DocumentInfo::default(),
            introspector,
        };

        let pdf = match typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()) {
            Ok(pdf) => pdf,
            Err(e) => {
                // Construct error node
                return Node::HtmlElement(CmarkHtmlElement {
                    tag: "div".to_string(),
                    attributes: vec![HtmlAttribute {
                        name: "class".to_string(),
                        value: "error".to_string(),
                    }],
                    children: vec![Node::Text(format!("Error create pdf frame to file: {e:?}"))],
                    self_closing: false,
                });
            }
        };

        if let Some(handler) = &self.feat.assets_handler {
            let file_id = self.frame_counter;
            self.frame_counter += 1;
            let file_name = format!("frame_{file_id}.pdf");

            if let Err(e) = handler.add_asset(Path::new(&file_name), pdf.as_slice()) {
                if self.feat.soft_error {
                    let data = base64::engine::general_purpose::STANDARD.encode(pdf.as_slice());
                    return Self::create_embedded_frame(&data);
                } else {
                    // Construct error node
                    return Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error writing frame to file: {}", e))],
                        self_closing: false,
                    });
                }
            }

            let data = base64::engine::general_purpose::STANDARD.encode(pdf.as_slice());
            return Node::Custom(Box::new(ExternalFrameNode {
                file_path: Path::new(&file_name).into(),
                alt_text: "typst-frame".to_string(),
                svg_data: data,
            }));
        }

        // Fall back to embedded mode if no external asset path is specified
        let data = base64::engine::general_purpose::STANDARD.encode(pdf.as_slice());
        Self::create_embedded_frame(&data)
    }

    /// Convert Typst frame to CommonMark node
    pub fn convert_svg_frame(&mut self, frame: &Frame) -> Node {
        if self.feat.remove_html {
            // todo: make error silent is not good.
            return Node::Text(String::new());
        }

        let svg = typst_svg::svg_frame(frame);

        if let Some(handler) = &self.feat.assets_handler {
            let file_id = self.frame_counter;
            self.frame_counter += 1;
            let file_name = format!("frame_{file_id}.svg");

            if let Err(e) = handler.add_asset(Path::new(&file_name), svg.as_bytes()) {
                if self.feat.soft_error {
                    let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
                    return Self::create_embedded_frame(&data);
                } else {
                    // Construct error node
                    return Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error writing frame to file: {}", e))],
                        self_closing: false,
                    });
                }
            }

            let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
            return Node::Custom(Box::new(ExternalFrameNode {
                file_path: Path::new(&file_name).into(),
                alt_text: "typst-frame".to_string(),
                svg_data: data,
            }));
        }

        // Fall back to embedded mode if no external asset path is specified
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
        Self::create_embedded_frame(&data)
    }

    /// Create embedded frame node
    fn create_embedded_frame(data: &str) -> Node {
        Node::HtmlElement(CmarkHtmlElement {
            tag: "img".to_string(),
            attributes: vec![
                HtmlAttribute {
                    name: "alt".to_string(),
                    value: "typst-block".to_string(),
                },
                HtmlAttribute {
                    name: "src".to_string(),
                    value: format!("data:image/svg+xml;base64,{data}"),
                },
            ],
            children: vec![],
            self_closing: true,
        })
    }
}
