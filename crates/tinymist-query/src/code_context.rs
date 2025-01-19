use serde::{Deserialize, Serialize};
use tinymist_world::ShadowApi;
use typst::foundations::{Bytes, IntoValue, StyleChain};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    analysis::analyze_expr,
    prelude::*,
    syntax::{interpret_mode_at, InterpretMode},
};

/// A query to get the mode at a specific position in a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextQuery {
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The position inside the text document.
        position: LspPosition,
    },
    /// Get the style at a specific position in a text document.
    StyleAt {
        /// The position inside the text document.
        position: LspPosition,
        /// Style to query
        style: Vec<String>,
    },
}

/// A response to a `InteractCodeContextQuery`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextResponse {
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The mode at the requested position.
        mode: InterpretMode,
    },
    /// Get the style at a specific position in a text document.
    StyleAt {
        /// The style at the requested position.
        style: Vec<Option<JsonValue>>,
    },
}

/// A request to get the code context of a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub struct InteractCodeContextRequest {
    /// The path to the text document.
    pub path: PathBuf,
    /// The queries to execute.
    pub query: Vec<Option<InteractCodeContextQuery>>,
}

impl SemanticRequest for InteractCodeContextRequest {
    type Response = Vec<Option<InteractCodeContextResponse>>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let mut responses = Vec::new();

        let source = ctx.source_by_path(&self.path).ok()?;

        for query in self.query {
            responses.push(query.and_then(|query| match query {
                InteractCodeContextQuery::ModeAt { position } => {
                    let cursor = ctx.to_typst_pos(position, &source)?;
                    let mode = Self::mode_at(&source, cursor)?;
                    Some(InteractCodeContextResponse::ModeAt { mode })
                }
                InteractCodeContextQuery::StyleAt { position, style } => {
                    let mut world = ctx.world().clone();
                    log::info!(
                        "style at position {position:?} . {style:?} when main is {:?}",
                        world.main()
                    );
                    let cursor = ctx.to_typst_pos(position, &source)?;
                    let root = LinkedNode::new(source.root());
                    let mut leaf = root.leaf_at_compat(cursor)?;
                    log::info!("style at leaf {leaf:?} . {style:?}");

                    if leaf.kind() != SyntaxKind::Text {
                        return None;
                    }

                    if matches!(leaf.parent_kind(), Some(SyntaxKind::Raw)) {
                        leaf = leaf.parent()?.clone();
                    }

                    let mode = Self::mode_at(&source, cursor);
                    if !matches!(
                        mode,
                        Some(InterpretMode::Code | InterpretMode::Markup | InterpretMode::Math)
                    ) {
                        leaf = leaf.parent()?.clone();
                    }
                    let mut mapped_source = source.clone();
                    let (with, offset) = match mode {
                        Some(InterpretMode::Code) => ("context text.font", 8),
                        _ => ("#context text.font", 10),
                    };
                    let start = leaf.range().start;
                    mapped_source.edit(leaf.range(), with);

                    let _ = world.map_shadow_by_id(
                        mapped_source.id(),
                        Bytes::from(mapped_source.text().as_bytes()),
                    );
                    world.source_db.take_state();

                    let root = LinkedNode::new(mapped_source.root());
                    let leaf = root.leaf_at_compat(start + offset)?;

                    log::info!("style at new_leaf {leaf:?} . {style:?}");

                    let mut cursor_styles = analyze_expr(&world, &leaf)
                        .iter()
                        .filter_map(|s| s.1.clone())
                        .collect::<Vec<_>>();
                    cursor_styles.sort_by_key(|x| x.as_slice().len());
                    log::info!("style at styles {cursor_styles:?} . {style:?}");
                    let cursor_style = cursor_styles.into_iter().last().unwrap_or_default();
                    let cursor_style = StyleChain::new(&cursor_style);

                    log::info!("style at style {cursor_style:?} . {style:?}");

                    let style = style
                        .iter()
                        .map(|style| Self::style_at(cursor_style, style))
                        .collect();
                    let _ = world.map_shadow_by_id(
                        mapped_source.id(),
                        Bytes::from(source.text().as_bytes()),
                    );

                    Some(InteractCodeContextResponse::StyleAt { style })
                }
            }));
        }

        Some(responses)
    }
}

impl InteractCodeContextRequest {
    fn mode_at(source: &Source, pos: usize) -> Option<InterpretMode> {
        // Smart special cases that is definitely at markup
        if pos == 0 || pos >= source.text().len() {
            return Some(InterpretMode::Markup);
        }

        // Get mode
        let root = LinkedNode::new(source.root());
        Some(interpret_mode_at(root.leaf_at_compat(pos).as_ref()))
    }

    fn style_at(cursor_style: StyleChain, style: &str) -> Option<JsonValue> {
        match style {
            "text.font" => {
                let font = typst::text::TextElem::font_in(cursor_style)
                    .clone()
                    .into_value();
                serde_json::to_value(font).ok()
            }
            _ => None,
        }
    }
}
