//! Media processing module, handles images, SVG and Frame media elements

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use std::sync::atomic::{AtomicUsize, Ordering};
use typst::layout::Frame;

use crate::common::ExternalFrameNode;

use super::core::HtmlToAstParser;

/// Media content parser
pub struct MediaParser;

impl MediaParser {
    /// Convert Typst frame to CommonMark node
    pub fn convert_frame(parser: &HtmlToAstParser, frame: &Frame) -> Node {
        let svg = typst_svg::svg_frame(frame);
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());

        if let Some(assets_path) = &parser.feat.assets_path {
            // Use a unique static counter to generate filenames
            static FRAME_COUNTER: AtomicUsize = AtomicUsize::new(0);
            let file_id = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
            let file_name = format!("frame_{}.svg", file_id);
            let file_path = assets_path.join(&file_name);

            if let Err(e) = std::fs::write(&file_path, svg.as_bytes()) {
                if parser.feat.soft_error {
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

            return Node::Custom(Box::new(ExternalFrameNode {
                file_path,
                alt_text: "typst-frame".to_string(),
                svg_data: data,
            }));
        }

        // Fall back to embedded mode if no external asset path is specified
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
