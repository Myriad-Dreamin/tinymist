//! Offline query the knowledge from the index.
//!
//! Types of Language Server Index Format (LSIF). LSIF is a standard format
//! for language servers or other programming tools to dump their knowledge
//! about a workspace.
//!
//! Based on <https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/>

use std::io::{BufRead, Read};

use tinymist_std::error::prelude::*;
use tinymist_std::hash::FxHashMap;

use crate::{CompilerQueryRequest, CompilerQueryResponse};

use super::protocol::*;
use lsp_types::Url;

/// The context for querying the index.
#[derive(Default)]
pub struct IndexQueryCtx {
    meta: Option<MetaData>,
    documents: FxHashMap<Url, Document>,
    graph: FxHashMap<i32, Vertex>,
    edges: FxHashMap<i32, Edge>,
}

impl IndexQueryCtx {
    /// Reads the index from a reader.
    pub fn read(reader: &mut impl BufRead) -> Result<Self> {
        let mut this = Self::default();

        for event in ReaderDecoder::new(reader) {
            this.update(event?)?;
        }

        Ok(this)
    }

    fn update(&mut self, event: Entry) -> Result<()> {
        match event.data {
            Element::Vertex(Vertex::MetaData(meta)) => {
                self.meta = Some(meta);
            }
            Element::Vertex(Vertex::Project(..)) => {
                bail!("project is not supported");
            }
            Element::Vertex(Vertex::Event(..)) => {
                bail!("event is not supported");
            }
            Element::Vertex(vertex) => {
                self.graph.insert(event.id, vertex);
            }
            Element::Edge(edge) => {
                self.edges.insert(edge.out_v(), edge);
            }
        }

        Ok(())
    }

    /// Requests the index for a compiler query.
    pub fn request(&mut self, request: CompilerQueryRequest) -> Option<CompilerQueryResponse> {
        let _ = self.documents;
        let _ = request;

        None
    }
}

// MetaData(MetaData),
// /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#the-project-vertex>
// Project(Project),
// Document(Document),
// /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#ranges>
// Range {
//     #[serde(flatten)]
//     range: Range,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     tag: Option<RangeTag>,
// },
// /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#result-set>
// ResultSet(ResultSet),
// Moniker(crate::Moniker),
// PackageInformation(PackageInformation),

// #[serde(rename = "$event")]
// Event(Event),

// DefinitionResult,
// DeclarationResult,
// TypeDefinitionResult,
// ReferenceResult,
// ImplementationResult,
// FoldingRangeResult {
//     result: Vec<crate::FoldingRange>,
// },
// HoverResult {
//     result: crate::Hover,
// },
// DocumentSymbolResult {
//     result: DocumentSymbolOrRangeBasedVec,
// },
// DocumentLinkResult {
//     result: Vec<crate::DocumentLink>,
// },
// DiagnosticResult {
//     result: Vec<crate::Diagnostic>,
// },

struct ReaderDecoder<R: Read> {
    reader: R,
}

impl<R: BufRead> ReaderDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: BufRead> Iterator for ReaderDecoder<R> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => return Some(Err(e).context("failed to read line")),
        }
        Some(serde_json::from_str(&line).context("failed to deserialize entry"))
    }
}
