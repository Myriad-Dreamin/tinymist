//! Media processing module, handles images, SVG and Frame media elements

use core::fmt;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use base64::Engine;
use cmark_writer::ast::{HtmlAttribute, HtmlElement as CmarkHtmlElement, Node};
use ecow::eco_format;
use tinymist_project::{base::ShadowApi, EntryReader, TaskInputs, MEMORY_MAIN_ENTRY};
use typst::{
    foundations::{Bytes, Dict, IntoValue},
    html::{HtmlElement, HtmlNode},
    layout::{Abs, Frame},
    utils::LazyHash,
    World,
};

use crate::{
    attributes::{md_attr, IdocAttr, TypliteAttrsParser},
    common::ExternalFrameNode,
    ColorTheme,
};

use super::core::HtmlToAstParser;

enum AssetUrl {
    /// Embedded Base64 SVG data
    Embedded(String),
    /// External file path
    External(PathBuf),
}

impl fmt::Display for AssetUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetUrl::Embedded(data) => write!(f, "data:image/svg+xml;base64,{data}"),
            // todo: correct relative path?
            AssetUrl::External(path) => write!(f, "{}", path.display()),
        }
    }
}

impl HtmlToAstParser {
    /// Convert Typst source to CommonMark node
    pub fn convert_source(&mut self, element: &HtmlElement) -> Node {
        if element.children.len() != 1 {
            // Construct error node
            return Node::HtmlElement(CmarkHtmlElement {
                tag: "div".to_string(),
                attributes: vec![HtmlAttribute {
                    name: "class".to_string(),
                    value: "error".to_string(),
                }],
                children: vec![Node::Text(format!(
                    "source contains not only one child: {}, whose attrs: {:?}",
                    element.children.len(),
                    element.attrs
                ))],
                self_closing: false,
            });
        }

        let Some(HtmlNode::Frame(frame)) = element.children.first() else {
            // todo: utils to remove duplicated error construction
            return Node::HtmlElement(CmarkHtmlElement {
                tag: "div".to_string(),
                attributes: vec![HtmlAttribute {
                    name: "class".to_string(),
                    value: "error".to_string(),
                }],
                children: vec![Node::Text(format!(
                    "source contains not a frame, but: {:?}",
                    element.children
                ))],
                self_closing: false,
            });
        };

        let svg = typst_svg::svg_frame(frame);
        let frame_url = match self.create_asset_url(&svg) {
            Ok(url) => url,
            Err(e) => {
                // Construct error node
                return Node::HtmlElement(CmarkHtmlElement {
                    tag: "div".to_string(),
                    attributes: vec![HtmlAttribute {
                        name: "class".to_string(),
                        value: "error".to_string(),
                    }],
                    children: vec![Node::Text(format!("Error creating source URL: {e}"))],
                    self_closing: false,
                });
            }
        };

        let media = element.attrs.0.iter().find_map(|(name, data)| {
            if *name == md_attr::media {
                Some(data.clone())
            } else {
                None
            }
        });

        Node::HtmlElement(CmarkHtmlElement {
            tag: "source".to_string(),
            attributes: vec![
                HtmlAttribute {
                    name: "media".to_string(),
                    value: media
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "all".to_string()),
                },
                HtmlAttribute {
                    name: "srcset".to_string(),
                    value: frame_url.to_string(),
                },
            ],
            children: vec![],
            self_closing: true,
        })
    }

    /// Convert Typst frame to CommonMark node
    pub fn convert_frame(&mut self, frame: &Frame) -> Node {
        if self.feat.remove_html {
            // todo: make error silent is not good.
            return Node::Text(String::new());
        }

        let svg = typst_svg::svg_frame(frame);
        self.convert_svg(svg)
    }

    fn convert_svg(&mut self, svg: String) -> Node {
        let frame_url = self.create_asset_url(&svg);

        match frame_url {
            Ok(url @ AssetUrl::Embedded(..)) => Self::create_embedded_frame(&url),
            Ok(AssetUrl::External(file_path)) => Node::Custom(Box::new(ExternalFrameNode {
                file_path,
                alt_text: "typst-frame".to_string(),
                svg,
            })),
            Err(e) => {
                if self.feat.soft_error {
                    let b64_data = Self::base64_url(&svg);
                    Self::create_embedded_frame(&b64_data)
                } else {
                    // Construct error node
                    Node::HtmlElement(CmarkHtmlElement {
                        tag: "div".to_string(),
                        attributes: vec![HtmlAttribute {
                            name: "class".to_string(),
                            value: "error".to_string(),
                        }],
                        children: vec![Node::Text(format!("Error creating frame URL: {}", e))],
                        self_closing: false,
                    })
                }
            }
        }
    }

    /// Create embedded frame node
    fn create_embedded_frame(url: &AssetUrl) -> Node {
        Node::HtmlElement(CmarkHtmlElement {
            tag: "img".to_string(),
            attributes: vec![
                HtmlAttribute {
                    name: "alt".to_string(),
                    value: "typst-block".to_string(),
                },
                HtmlAttribute {
                    name: "src".to_string(),
                    value: url.to_string(),
                },
            ],
            children: vec![],
            self_closing: true,
        })
    }

    /// Convert asset to asset url
    fn create_asset_url(&mut self, svg: &str) -> crate::Result<AssetUrl> {
        if let Some(assets_path) = &self.feat.assets_path {
            let file_id = self.asset_counter;
            self.asset_counter += 1;
            let file_name = format!("frame_{file_id}.svg");
            let file_path = assets_path.join(&file_name);

            std::fs::write(&file_path, svg.as_bytes())?;
            return Ok(AssetUrl::External(file_path));
        }

        // Fall back to embedded mode if no external asset path is specified
        Ok(Self::base64_url(svg))
    }

    /// Create embedded frame node
    fn base64_url(data: &str) -> AssetUrl {
        AssetUrl::Embedded(base64::engine::general_purpose::STANDARD.encode(data.as_bytes()))
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
        self.convert_svg(svg)
    }
}
