//! Borrowed from `lsp-types` crate and modified to fit our need.
//! Types of Language Server Index Format (LSIF). LSIF is a standard format
//! for language servers or other programming tools to dump their knowledge
//! about a workspace.
//!
//! Based on <https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/>

#![allow(missing_docs)]
// todo: large_enum_variant

use ecow::EcoString;
use lsp_types::{Range, SemanticTokens, Url};
use serde::{Deserialize, Serialize};

pub type Id = i32;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LocationOrRangeId {
    Location(lsp_types::Location),
    RangeId(Id),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: Id,
    #[serde(flatten)]
    pub data: Element,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum Element {
    Vertex(Vertex),
    Edge(Edge),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(default = "Default::default")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum Encoding {
    /// Currently only 'utf-16' is supported due to the limitations in LSP.
    #[serde(rename = "utf-16")]
    Utf16,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RangeBasedDocumentSymbol {
    pub id: Id,
    #[serde(default = "Default::default")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<RangeBasedDocumentSymbol>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum DocumentSymbolOrRangeBasedVec {
    DocumentSymbol(Vec<lsp_types::DocumentSymbol>),
    RangeBased(Vec<RangeBasedDocumentSymbol>),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionTag {
    /// The text covered by the range     
    text: String,
    /// The symbol kind.
    kind: lsp_types::SymbolKind,
    /// Indicates if this symbol is deprecated.
    #[serde(default)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    deprecated: bool,
    /// The full range of the definition not including leading/trailing
    /// whitespace but everything else, e.g comments and code.
    /// The range must be included in fullRange.
    full_range: Range,
    /// Optional detail information for the definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationTag {
    /// The text covered by the range     
    text: String,
    /// The symbol kind.
    kind: lsp_types::SymbolKind,
    /// Indicates if this symbol is deprecated.
    #[serde(default)]
    deprecated: bool,
    /// The full range of the definition not including leading/trailing
    /// whitespace but everything else, e.g comments and code.
    /// The range must be included in fullRange.
    full_range: Range,
    /// Optional detail information for the definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceTag {
    text: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownTag {
    text: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum RangeTag {
    Definition(DefinitionTag),
    Declaration(DeclarationTag),
    Reference(ReferenceTag),
    Unknown(UnknownTag),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "label")]
pub enum Vertex {
    MetaData(MetaData),
    /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#the-project-vertex>
    Project(Project),
    Document(Document),
    /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#ranges>
    Range {
        #[serde(flatten)]
        range: Range,
        #[serde(skip_serializing_if = "Option::is_none")]
        tag: Option<RangeTag>,
    },
    /// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#result-set>
    ResultSet(ResultSet),
    Moniker(lsp_types::Moniker),
    PackageInformation(PackageInformation),

    #[serde(rename = "$event")]
    Event(Event),

    DefinitionResult,
    DeclarationResult,
    TypeDefinitionResult,
    ReferenceResult,
    ImplementationResult,
    FoldingRangeResult {
        result: Vec<lsp_types::FoldingRange>,
    },
    SemanticTokensResult {
        result: SemanticTokens,
    },
    HoverResult {
        result: lsp_types::Hover,
    },
    DocumentSymbolResult {
        result: DocumentSymbolOrRangeBasedVec,
    },
    DocumentLinkResult {
        result: Vec<lsp_types::DocumentLink>,
    },
    DiagnosticResult {
        result: Vec<lsp_types::Diagnostic>,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Begin,
    End,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventScope {
    Document,
    Project,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub scope: EventScope,
    pub data: Id,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "label")]
pub enum Edge {
    Contains(EdgeDataMultiIn),
    Moniker(EdgeData),
    NextMoniker(EdgeData),
    Next(EdgeData),
    PackageInformation(EdgeData),
    Item(Item),

    // Methods
    #[serde(rename = "textDocument/definition")]
    Definition(EdgeData),
    #[serde(rename = "textDocument/declaration")]
    Declaration(EdgeData),
    #[serde(rename = "textDocument/hover")]
    Hover(EdgeData),
    #[serde(rename = "textDocument/references")]
    References(EdgeData),
    #[serde(rename = "textDocument/implementation")]
    Implementation(EdgeData),
    #[serde(rename = "textDocument/typeDefinition")]
    TypeDefinition(EdgeData),
    #[serde(rename = "textDocument/foldingRange")]
    FoldingRange(EdgeData),
    #[serde(rename = "textDocument/documentLink")]
    DocumentLink(EdgeData),
    #[serde(rename = "textDocument/documentSymbol")]
    DocumentSymbol(EdgeData),
    #[serde(rename = "textDocument/diagnostic")]
    Diagnostic(EdgeData),
    #[serde(rename = "textDocument/semanticTokens")]
    SemanticTokens(EdgeData),
}

impl Edge {
    pub fn out_v(&self) -> Id {
        match self {
            Edge::Contains(data) => data.out_v,
            Edge::Item(item) => item.edge_data.out_v,
            Edge::Moniker(data)
            | Edge::NextMoniker(data)
            | Edge::Next(data)
            | Edge::PackageInformation(data)
            | Edge::Definition(data)
            | Edge::Declaration(data)
            | Edge::Hover(data)
            | Edge::References(data)
            | Edge::Implementation(data)
            | Edge::TypeDefinition(data)
            | Edge::FoldingRange(data)
            | Edge::DocumentLink(data)
            | Edge::DocumentSymbol(data)
            | Edge::Diagnostic(data)
            | Edge::SemanticTokens(data) => data.out_v,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeData {
    pub in_v: Id,
    pub out_v: Id,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDataMultiIn {
    pub in_vs: Vec<Id>,
    pub out_v: Id,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DefinitionResultType {
    Scalar(LocationOrRangeId),
    Array(LocationOrRangeId),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Declarations,
    Definitions,
    References,
    ReferenceResults,
    ImplementationResults,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub document: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<ItemKind>,
    #[serde(flatten)]
    pub edge_data: EdgeDataMultiIn,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub uri: Url,
    pub language_id: EcoString,
}

/// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#result-set>
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// <https://github.com/Microsoft/language-server-protocol/blob/master/indexFormat/specification.md#the-project-vertex>
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaData {
    /// The version of the LSIF format using semver notation. See <https://semver.org/>. Please note
    /// the version numbers starting with 0 don't adhere to semver and adopters
    /// have to assume that each new version is breaking.
    pub version: String,

    /// The project root (in form of an URI) used to compute this dump.
    pub project_root: Url,

    /// The string encoding used to compute line and character values in
    /// positions and ranges.
    pub position_encoding: Encoding,

    /// Information about the tool that created the dump
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_info: Option<ToolInfo>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub r#type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInformation {
    pub name: String,
    pub manager: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<Repository>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
