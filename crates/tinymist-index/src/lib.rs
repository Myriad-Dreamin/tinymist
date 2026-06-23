//! # tinymist-index
//!
//! This crate provides a semantic index implementation for Typst.
//!
//! ## Documentation
//!
//! See [Crate Docs](https://myriad-dreamin.github.io/tinymist/rs/tinymist_index/index.html).

#![cfg_attr(
    all(feature = "typst-plugin", target_arch = "wasm32"),
    allow(missing_docs)
)]

use std::sync::{Mutex, OnceLock};

use lsp_types::{GotoDefinitionParams, HoverParams};
use tinymist_query::{
    CompilerQueryRequest, CompilerQueryResponse, GotoDefinitionRequest, HoverRequest,
    index::scip_query::{ScipPublicSymbol, ScipQueryCtx},
    url_to_path,
};
#[cfg(all(feature = "typst-plugin", target_arch = "wasm32"))]
use wasm_minimal_protocol::*;

#[cfg(all(feature = "typst-plugin", target_arch = "wasm32"))]
initiate_protocol!();

type StrResult<T> = Result<T, String>;

static INDEX: OnceLock<Mutex<IndexCtx>> = OnceLock::new();

struct IndexCtx {
    /// The database for the index.
    index: ScipQueryCtx,
}

enum IndexRequest {
    Compiler(CompilerQueryRequest),
    PublicSymbols(String),
}

enum IndexResponse {
    Compiler(Option<CompilerQueryResponse>),
    PublicSymbols(Vec<ScipPublicSymbol>),
}

impl IndexCtx {
    fn request(&mut self, request: IndexRequest) -> IndexResponse {
        match request {
            IndexRequest::Compiler(request) => IndexResponse::Compiler(self.index.request(request)),
            IndexRequest::PublicSymbols(path) => {
                IndexResponse::PublicSymbols(self.index.public_symbols(&path))
            }
        }
    }
}

impl IndexResponse {
    fn to_bytes(&self) -> StrResult<Vec<u8>> {
        match self {
            Self::Compiler(response) => serde_json::to_vec(response).map_err(to_string),
            Self::PublicSymbols(response) => serde_json::to_vec(response).map_err(to_string),
        }
    }
}

/// Creates an index.
#[cfg_attr(all(feature = "typst-plugin", target_arch = "wasm32"), wasm_func)]
pub fn create_index(db: &[u8], opts: &[u8]) -> StrResult<Vec<u8>> {
    create_index_inner(db, opts).map(|_| vec![])
}

/// Queries the index.
#[cfg_attr(all(feature = "typst-plugin", target_arch = "wasm32"), wasm_func)]
pub fn query_index(kind: &[u8], request: &[u8]) -> StrResult<Vec<u8>> {
    let kind = str::from_utf8(kind).map_err(to_string)?;
    let request = parse_request(kind, request)?;
    let response = {
        let mut index = INDEX.get().ok_or("index was not created")?.lock().unwrap();
        index.request(request)
    };
    response.to_bytes()
}

fn parse_request(kind: &str, request: &[u8]) -> StrResult<IndexRequest> {
    Ok(match kind {
        "textDocument/hover" => IndexRequest::Compiler(parse_hover_request(request)?),
        "textDocument/definition" => {
            IndexRequest::Compiler(parse_goto_definition_request(request)?)
        }
        "public_symbols" => {
            IndexRequest::PublicSymbols(serde_json::from_slice(request).map_err(to_string)?)
        }
        kind => Err(format!("unknown request kind: {kind}"))?,
    })
}

fn parse_hover_request(request: &[u8]) -> StrResult<CompilerQueryRequest> {
    if let Ok(symbol) = serde_json::from_slice(request) {
        return Ok(CompilerQueryRequest::HoverSymbol(symbol));
    }

    let req: HoverParams = serde_json::from_slice(request).map_err(to_string)?;
    Ok(CompilerQueryRequest::Hover(HoverRequest {
        path: url_to_path(&req.text_document_position_params.text_document.uri),
        position: req.text_document_position_params.position,
    }))
}

fn parse_goto_definition_request(request: &[u8]) -> StrResult<CompilerQueryRequest> {
    if let Ok(symbol) = serde_json::from_slice(request) {
        return Ok(CompilerQueryRequest::GotoDefinitionSymbol(symbol));
    }

    let req: GotoDefinitionParams = serde_json::from_slice(request).map_err(to_string)?;
    Ok(CompilerQueryRequest::GotoDefinition(
        GotoDefinitionRequest {
            path: url_to_path(&req.text_document_position_params.text_document.uri),
            position: req.text_document_position_params.position,
        },
    ))
}

/// Creates an index.
fn create_index_inner(db: &[u8], opts: &[u8]) -> StrResult<()> {
    let index = ScipQueryCtx::read(db).map_err(to_string)?;
    let _ = opts;

    let index = INDEX.set(Mutex::new(IndexCtx { index }));
    if index.is_err() {
        Err("index was already created")?;
    }
    Ok(())
}

fn to_string(i: impl ToString) -> String {
    i.to_string()
}
