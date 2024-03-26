// todo: remove this
#![allow(missing_docs)]

use lsp_types::{
    Registration, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    Unregistration,
};
use strum::IntoEnumIterator;

use crate::{Modifier, TokenType};

fn get_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenType::iter()
            .filter(|e| *e != TokenType::None)
            .map(Into::into)
            .collect(),
        token_modifiers: Modifier::iter().map(Into::into).collect(),
    }
}

const SEMANTIC_TOKENS_REGISTRATION_ID: &str = "semantic_tokens";
const SEMANTIC_TOKENS_METHOD_ID: &str = "textDocument/semanticTokens";

pub fn get_semantic_tokens_registration(options: SemanticTokensOptions) -> Registration {
    Registration {
        id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
        method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
        register_options: Some(
            serde_json::to_value(options)
                .expect("semantic tokens options should be representable as JSON value"),
        ),
    }
}

pub fn get_semantic_tokens_unregistration() -> Unregistration {
    Unregistration {
        id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
        method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
    }
}

pub fn get_semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: get_legend(),
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
        ..Default::default()
    }
}
