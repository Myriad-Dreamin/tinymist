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

use std::{
    io::BufReader,
    sync::{Mutex, OnceLock},
};

use lsp_types::{GotoDefinitionParams, HoverParams};
use tinymist_query::{
    CompilerQueryRequest, GotoDefinitionRequest, HoverRequest, index::query::IndexQueryCtx,
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
    index: IndexQueryCtx,
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
        index.index.request(request)
    };
    match response {
        None => Ok("null".as_bytes().to_vec()),
        Some(tinymist_query::CompilerQueryResponse::Hover(response)) => {
            serde_json::to_vec(&response).map_err(to_string)
        }
        Some(tinymist_query::CompilerQueryResponse::GotoDefinition(response)) => {
            serde_json::to_vec(&response).map_err(to_string)
        }
        _ => Err("unknown response kind".to_owned())?,
    }
}

fn parse_request(kind: &str, request: &[u8]) -> StrResult<CompilerQueryRequest> {
    Ok(match kind {
        "hover" => CompilerQueryRequest::Hover({
            let req: HoverParams = serde_json::from_slice(request).map_err(to_string)?;
            HoverRequest {
                path: url_to_path(&req.text_document_position_params.text_document.uri),
                position: req.text_document_position_params.position,
            }
        }),
        "goto_definition" => CompilerQueryRequest::GotoDefinition({
            let req: GotoDefinitionParams = serde_json::from_slice(request).map_err(to_string)?;
            GotoDefinitionRequest {
                path: url_to_path(&req.text_document_position_params.text_document.uri),
                position: req.text_document_position_params.position,
            }
        }),
        kind => Err(format!("unknown request kind: {kind}"))?,
    })
}

/// Creates an index.
fn create_index_inner(db: &[u8], opts: &[u8]) -> StrResult<()> {
    let index = IndexQueryCtx::read(&mut BufReader::new(db)).map_err(to_string)?;
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
