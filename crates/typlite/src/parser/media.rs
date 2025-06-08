//! Media processing module, handles images, SVG and Frame media elements

use std::sync::{Arc, LazyLock};

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use ecow::eco_format;
use tinymist_project::{base::ShadowApi, EntryReader, TaskInputs, MEMORY_MAIN_ENTRY};
use typst::{
    foundations::{Bytes, Dict, IntoValue},
    html::HtmlElement,
    layout::{Abs, Frame},
    utils::LazyHash,
    World,
};

use crate::{
    attributes::{IdocAttr, TypliteAttrsParser},
    common::ExternalFrameNode,
    ColorTheme,
};

use super::core::HtmlToAstParser;

impl HtmlToAstParser {
    /// Convert Typst frame to CommonMark node
    pub fn convert_frame(&mut self, frame: &Frame) -> Node {
        if self.feat.remove_html {
            // todo: make error silent is not good.
            return Node::Text(String::new());
        }

        let svg = typst_svg::svg_frame(frame);
        self.convert_svg(&svg)
    }

    fn convert_svg(&mut self, svg: &str) -> Node {
        let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());

        if let Some(assets_path) = &self.feat.assets_path {
            let file_id = self.frame_counter;
            self.frame_counter += 1;
            let file_name = format!("frame_{file_id}.svg");
            let file_path = assets_path.join(&file_name);

            if let Err(e) = std::fs::write(&file_path, svg.as_bytes()) {
                if self.feat.soft_error {
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

    /// Convert Typst inline document to CommonMark node
    pub fn convert_idoc(&mut self, element: &HtmlElement) -> Node {
        static DARK_THEME_INPUT: LazyLock<Arc<LazyHash<Dict>>> = LazyLock::new(|| {
            Arc::new(LazyHash::new(Dict::from_iter(std::iter::once((
                "x-color-theme".into(),
                "dark".into_value(),
            )))))
        });

        if self.feat.remove_html {
            eprintln!("Removing idoc element due to remove_html feature");
            // todo: make error silent is not good.
            return Node::Text(String::new());
        }
        let attrs = match IdocAttr::parse(&element.attrs) {
            Ok(attrs) => attrs,
            Err(e) => {
                if self.feat.soft_error {
                    return Node::Text(format!("Error parsing idoc attributes: {e}"));
                } else {
                    // Construct error node
                    return Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error parsing idoc attributes: {e}"))],
                        self_closing: false,
                    });
                }
            }
        };

        let src = attrs.src;
        let mode = attrs.mode;

        let mut world = self.world.clone().task(TaskInputs {
            entry: Some(
                self.world
                    .entry_state()
                    .select_in_workspace(MEMORY_MAIN_ENTRY.vpath().as_rooted_path()),
            ),
            inputs: match self.feat.color_theme {
                Some(ColorTheme::Dark) => Some(DARK_THEME_INPUT.clone()),
                None | Some(ColorTheme::Light) => None,
            },
        });
        // todo: cost some performance.
        world.take_db();

        let main = world.main();

        const PRELUDE: &str = r##"#set page(width: auto, height: auto, margin: (y: 0.45em, rest: 0em), fill: none);
            #set text(fill: rgb("#c0caf5")) if sys.inputs.at("x-color-theme", default: none) == "dark";"##;
        world
            .map_shadow_by_id(
                main,
                Bytes::from_string(match mode.as_str() {
                    "code" => eco_format!("{PRELUDE}#{{{src}}}"),
                    "math" => eco_format!("{PRELUDE}${src}$"),
                    "markup" => eco_format!("{PRELUDE}#[{}]", src),
                    // todo check mode
                    //  "markup" |
                    _ => eco_format!("{PRELUDE}#[{}]", src),
                }),
            )
            .unwrap();

        let doc = typst::compile(&world);
        let doc = match doc.output {
            Ok(doc) => doc,
            Err(e) => {
                if self.feat.soft_error {
                    return Node::Text(format!("Error compiling idoc: {e:?}"));
                } else {
                    // Construct error node
                    return Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error compiling idoc: {e:?}"))],
                        self_closing: false,
                    });
                }
            }
        };

        let svg = typst_svg::svg_merged(&doc, Abs::zero());
        self.convert_svg(&svg)
    }
}
