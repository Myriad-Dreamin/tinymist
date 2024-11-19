use serde::{Deserialize, Serialize};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    prelude::*,
    syntax::{interpret_mode_at, InterpretMode},
    SyntaxRequest,
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
}

/// A response to a `InteractCodeContextQuery`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InteractCodeContextResponse {
    /// Get the mode at a specific position in a text document.
    ModeAt {
        /// The mode at the requested position.
        mode: InterpretMode,
    },
}

/// A request to get the code context of a text document.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub struct InteractCodeContextRequest {
    /// The path to the text document.
    pub path: PathBuf,
    /// The queries to execute.
    pub query: Vec<InteractCodeContextQuery>,
}

impl SyntaxRequest for InteractCodeContextRequest {
    type Response = Vec<InteractCodeContextResponse>;

    fn request(
        self,
        source: &Source,
        positing_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let mut responses = Vec::new();

        for query in self.query {
            match query {
                InteractCodeContextQuery::ModeAt { position } => {
                    let mode = Self::mode_at(source, positing_encoding, position)?;
                    responses.push(InteractCodeContextResponse::ModeAt { mode });
                }
            }
        }

        Some(responses)
    }
}

impl InteractCodeContextRequest {
    fn mode_at(
        source: &Source,
        positing_encoding: PositionEncoding,
        position: LspPosition,
    ) -> Option<InterpretMode> {
        let pos = lsp_to_typst::position(position, positing_encoding, source)?;
        // Smart special cases that is definitely at markup
        if pos == 0 || pos >= source.text().len() {
            return Some(InterpretMode::Markup);
        }

        // Get mode
        let root = LinkedNode::new(source.root());
        Some(interpret_mode_at(root.leaf_at_compat(pos).as_ref()))
    }
}
