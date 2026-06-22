//! Offline query the knowledge from the index.
//!
//! Types of Language Server Index Format (LSIF). LSIF is a standard format
//! for language servers or other programming tools to dump their knowledge
//! about a workspace.
//!
//! Based on <https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/>

use std::io::{BufRead, Read};

use tinymist_std::error::prelude::*;
use tinymist_std::hash::{FxHashMap, FxHashSet};

use crate::{CompilerQueryRequest, CompilerQueryResponse, GotoDefinitionRequest, path_to_url};

use super::protocol::*;
use lsp_types::{GotoDefinitionResponse, LocationLink, Position, Range, Url};

/// The context for querying the index.
#[derive(Default)]
pub struct IndexQueryCtx {
    meta: Option<MetaData>,
    document_by_uri: FxHashMap<Url, Id>,
    document_ranges: FxHashMap<Id, Vec<Id>>,
    definition_ranges: FxHashSet<Id>,
    graph: FxHashMap<i32, Vertex>,
    edges: FxHashMap<i32, Vec<Edge>>,
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
            Element::Vertex(Vertex::Document(document)) => {
                self.document_by_uri.insert(document.uri.clone(), event.id);
                self.graph.insert(event.id, Vertex::Document(document));
            }
            Element::Vertex(vertex) => {
                self.graph.insert(event.id, vertex);
            }
            Element::Edge(edge) => {
                match &edge {
                    Edge::Contains(data) => {
                        self.document_ranges
                            .entry(data.out_v)
                            .or_default()
                            .extend(data.in_vs.iter().copied());
                    }
                    Edge::Item(item) if self.is_definition_result(item.edge_data.out_v) => {
                        self.definition_ranges
                            .extend(item.edge_data.in_vs.iter().copied());
                    }
                    _ => {}
                }
                self.edges.entry(edge.out_v()).or_default().push(edge);
            }
        }

        Ok(())
    }

    /// Requests the index for a compiler query.
    pub fn request(&mut self, request: CompilerQueryRequest) -> Option<CompilerQueryResponse> {
        match request {
            CompilerQueryRequest::GotoDefinition(request) => Some(
                CompilerQueryResponse::GotoDefinition(self.goto_definition(request)),
            ),
            _ => None,
        }
    }

    fn goto_definition(&self, request: GotoDefinitionRequest) -> Option<GotoDefinitionResponse> {
        let uri = path_to_url(&request.path).ok()?;
        let doc_id = *self.document_by_uri.get(&uri)?;
        let source_range_id = self.find_range(doc_id, request.position)?;
        let origin_selection_range = self.range_of(source_range_id)?;
        let result_id = match self.next_result(source_range_id) {
            Some(result_id) => result_id,
            None => {
                return self.definition_target_link(
                    doc_id,
                    source_range_id,
                    origin_selection_range,
                );
            }
        };
        let definition_result_id = self.definition_result(result_id)?;
        let links = self.definition_links(definition_result_id, origin_selection_range);

        if links.is_empty() {
            None
        } else {
            Some(GotoDefinitionResponse::Link(links))
        }
    }

    fn is_definition_result(&self, id: Id) -> bool {
        matches!(self.graph.get(&id), Some(Vertex::DefinitionResult))
    }

    fn find_range(&self, document_id: Id, position: Position) -> Option<Id> {
        self.document_ranges
            .get(&document_id)?
            .iter()
            .filter_map(|id| Some((*id, self.range_of(*id)?)))
            .filter(|(_, range)| contains_position(range, position))
            .min_by_key(|(_, range)| range_len_key(range))
            .map(|(id, _)| id)
    }

    fn range_of(&self, id: Id) -> Option<Range> {
        match self.graph.get(&id)? {
            Vertex::Range { range, .. } => Some(*range),
            _ => None,
        }
    }

    fn document_uri(&self, id: Id) -> Option<Url> {
        match self.graph.get(&id)? {
            Vertex::Document(document) => Some(document.uri.clone()),
            _ => None,
        }
    }

    fn definition_target_link(
        &self,
        document_id: Id,
        range_id: Id,
        target_range: Range,
    ) -> Option<GotoDefinitionResponse> {
        if !self.definition_ranges.contains(&range_id) {
            return None;
        }

        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(target_range),
            target_uri: self.document_uri(document_id)?,
            target_range,
            target_selection_range: target_range,
        }]))
    }

    fn next_result(&self, range_id: Id) -> Option<Id> {
        self.edges
            .get(&range_id)?
            .iter()
            .find_map(|edge| match edge {
                Edge::Next(data) => Some(data.in_v),
                _ => None,
            })
    }

    fn definition_result(&self, result_id: Id) -> Option<Id> {
        self.edges
            .get(&result_id)?
            .iter()
            .find_map(|edge| match edge {
                Edge::Definition(data) => Some(data.in_v),
                _ => None,
            })
    }

    fn definition_links(
        &self,
        definition_result_id: Id,
        origin_selection_range: Range,
    ) -> Vec<LocationLink> {
        self.edges
            .get(&definition_result_id)
            .into_iter()
            .flatten()
            .filter_map(|edge| match edge {
                Edge::Item(item) => Some(item),
                _ => None,
            })
            .filter_map(|item| {
                let target_uri = self.document_uri(item.document)?;
                Some(item.edge_data.in_vs.iter().filter_map(move |range_id| {
                    let target_selection_range = self.range_of(*range_id)?;
                    Some(LocationLink {
                        origin_selection_range: Some(origin_selection_range),
                        target_uri: target_uri.clone(),
                        target_range: target_selection_range,
                        target_selection_range,
                    })
                }))
            })
            .flatten()
            .collect()
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

fn contains_position(range: &Range, position: Position) -> bool {
    position_after_or_eq(position, range.start) && position_before(position, range.end)
}

fn position_after_or_eq(a: Position, b: Position) -> bool {
    (a.line, a.character) >= (b.line, b.character)
}

fn position_before(a: Position, b: Position) -> bool {
    (a.line, a.character) < (b.line, b.character)
}

fn range_len_key(range: &Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}
