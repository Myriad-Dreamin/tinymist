//! # tinymist-query
//!
//! **Note: this crate is under development. it currently doesn't ensure stable
//! APIs, and heavily depending on some unstable crates.**
//!
//! This crate provides a set of APIs to query the information about the source
//! code. Currently it provides:
//! + language queries defined by the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

mod adt;
pub mod analysis;
pub mod syntax;
mod upstream;

pub(crate) mod diagnostics;

use std::sync::Arc;

pub use analysis::AnalysisContext;
use typst::{model::Document as TypstDocument, syntax::Source};

pub use diagnostics::*;
pub(crate) mod code_lens;
pub use code_lens::*;
pub(crate) mod completion;
pub use completion::*;
pub(crate) mod document_symbol;
pub use document_symbol::*;
pub(crate) mod document_metrics;
pub use document_metrics::*;
pub(crate) mod folding_range;
pub use folding_range::*;
pub(crate) mod goto_declaration;
pub use goto_declaration::*;
pub(crate) mod goto_definition;
pub use goto_definition::*;
pub(crate) mod hover;
pub use hover::*;
pub(crate) mod inlay_hint;
pub use inlay_hint::*;
pub(crate) mod rename;
pub use rename::*;
pub(crate) mod selection_range;
pub use selection_range::*;
pub(crate) mod semantic_tokens;
pub use semantic_tokens::*;
pub(crate) mod semantic_tokens_full;
pub use semantic_tokens_full::*;
pub(crate) mod semantic_tokens_delta;
pub use semantic_tokens_delta::*;
pub(crate) mod signature_help;
pub use signature_help::*;
pub(crate) mod symbol;
pub use symbol::*;
pub(crate) mod prepare_rename;
pub use prepare_rename::*;
pub(crate) mod references;
pub use references::*;

pub mod lsp_typst_boundary;
pub use lsp_typst_boundary::*;
pub(crate) mod lsp_features;
pub use lsp_features::*;

mod prelude;

/// A compiled document with an self-incremented logical version.
#[derive(Debug, Clone)]
pub struct VersionedDocument {
    /// The version of the document.
    pub version: usize,
    /// The compiled document.
    pub document: Arc<TypstDocument>,
}

/// A request handler with given syntax information.
pub trait SyntaxRequest {
    /// The response type of the request.
    type Response;

    /// Request the information from the given source.
    fn request(
        self,
        source: &Source,
        positing_encoding: PositionEncoding,
    ) -> Option<Self::Response>;
}

/// A request handler with given (semantic) analysis context.
pub trait SemanticRequest {
    /// The response type of the request.
    type Response;

    /// Request the information from the given context.
    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response>;
}

/// A request handler with given (semantic) analysis context and a versioned
/// document.
pub trait StatefulRequest {
    /// The response type of the request.
    type Response;

    /// Request the information from the given context.
    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response>;
}

#[allow(missing_docs)]
mod polymorphic {
    use lsp_types::TextEdit;
    use serde::{Deserialize, Serialize};
    use typst::foundations::Dict;

    use super::prelude::*;
    use super::*;

    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub enum PageSelection {
        #[serde(rename = "first")]
        First,
        #[serde(rename = "merged")]
        Merged,
    }

    #[derive(Debug, Clone)]
    pub enum ExportKind {
        Pdf,
        WordCount,
        Svg { page: PageSelection },
        Png { page: PageSelection },
    }

    impl ExportKind {
        pub fn extension(&self) -> &str {
            match self {
                Self::Pdf => "pdf",
                Self::WordCount => "txt",
                Self::Svg { .. } => "svg",
                Self::Png { .. } => "png",
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct OnExportRequest {
        pub path: PathBuf,
        pub kind: ExportKind,
    }

    #[derive(Debug, Clone)]
    pub struct OnSaveExportRequest {
        pub path: PathBuf,
    }

    #[derive(Debug, Clone)]
    pub struct FormattingRequest {
        /// The path of the document to get semantic tokens for.
        pub path: PathBuf,
    }

    #[derive(Debug, Clone)]
    pub struct ServerInfoRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServerInfoReponse {
        pub root: Option<PathBuf>,
        #[serde(rename = "fontPaths")]
        pub font_paths: Vec<PathBuf>,
        pub inputs: Dict,
        #[serde(rename = "estimatedMemoryUsage")]
        pub estimated_memory_usage: HashMap<String, usize>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FoldRequestFeature {
        PinnedFirst,
        Unique,
        Mergable,
        ContextFreeUnique,
    }

    #[derive(Debug, Clone)]
    pub enum CompilerQueryRequest {
        OnExport(OnExportRequest),
        OnSaveExport(OnSaveExportRequest),
        Hover(HoverRequest),
        GotoDefinition(GotoDefinitionRequest),
        GotoDeclaration(GotoDeclarationRequest),
        References(ReferencesRequest),
        InlayHint(InlayHintRequest),
        CodeLens(CodeLensRequest),
        Completion(CompletionRequest),
        SignatureHelp(SignatureHelpRequest),
        Rename(RenameRequest),
        PrepareRename(PrepareRenameRequest),
        DocumentSymbol(DocumentSymbolRequest),
        Symbol(SymbolRequest),
        SemanticTokensFull(SemanticTokensFullRequest),
        SemanticTokensDelta(SemanticTokensDeltaRequest),
        Formatting(FormattingRequest),
        FoldingRange(FoldingRangeRequest),
        SelectionRange(SelectionRangeRequest),

        DocumentMetrics(DocumentMetricsRequest),
        ServerInfo(ServerInfoRequest),
    }

    impl CompilerQueryRequest {
        pub fn fold_feature(&self) -> FoldRequestFeature {
            use FoldRequestFeature::*;
            match self {
                CompilerQueryRequest::OnExport(..) => Mergable,
                CompilerQueryRequest::OnSaveExport(..) => Mergable,
                CompilerQueryRequest::Hover(..) => PinnedFirst,
                CompilerQueryRequest::GotoDefinition(..) => PinnedFirst,
                CompilerQueryRequest::GotoDeclaration(..) => PinnedFirst,
                CompilerQueryRequest::References(..) => PinnedFirst,
                CompilerQueryRequest::InlayHint(..) => Unique,
                CompilerQueryRequest::CodeLens(..) => Unique,
                CompilerQueryRequest::Completion(..) => Mergable,
                CompilerQueryRequest::SignatureHelp(..) => PinnedFirst,
                CompilerQueryRequest::Rename(..) => Mergable,
                CompilerQueryRequest::PrepareRename(..) => Mergable,
                CompilerQueryRequest::DocumentSymbol(..) => ContextFreeUnique,
                CompilerQueryRequest::Symbol(..) => Mergable,
                CompilerQueryRequest::SemanticTokensFull(..) => ContextFreeUnique,
                CompilerQueryRequest::SemanticTokensDelta(..) => ContextFreeUnique,
                CompilerQueryRequest::Formatting(..) => ContextFreeUnique,
                CompilerQueryRequest::FoldingRange(..) => ContextFreeUnique,
                CompilerQueryRequest::SelectionRange(..) => ContextFreeUnique,

                CompilerQueryRequest::DocumentMetrics(..) => PinnedFirst,
                CompilerQueryRequest::ServerInfo(..) => Mergable,
            }
        }

        pub fn associated_path(&self) -> Option<&Path> {
            Some(match self {
                CompilerQueryRequest::OnExport(..) => return None,
                CompilerQueryRequest::OnSaveExport(req) => &req.path,
                CompilerQueryRequest::Hover(req) => &req.path,
                CompilerQueryRequest::GotoDefinition(req) => &req.path,
                CompilerQueryRequest::GotoDeclaration(req) => &req.path,
                CompilerQueryRequest::References(req) => &req.path,
                CompilerQueryRequest::InlayHint(req) => &req.path,
                CompilerQueryRequest::CodeLens(req) => &req.path,
                CompilerQueryRequest::Completion(req) => &req.path,
                CompilerQueryRequest::SignatureHelp(req) => &req.path,
                CompilerQueryRequest::Rename(req) => &req.path,
                CompilerQueryRequest::PrepareRename(req) => &req.path,
                CompilerQueryRequest::DocumentSymbol(req) => &req.path,
                CompilerQueryRequest::Symbol(..) => return None,
                CompilerQueryRequest::SemanticTokensFull(req) => &req.path,
                CompilerQueryRequest::SemanticTokensDelta(req) => &req.path,
                CompilerQueryRequest::Formatting(req) => &req.path,
                CompilerQueryRequest::FoldingRange(req) => &req.path,
                CompilerQueryRequest::SelectionRange(req) => &req.path,

                CompilerQueryRequest::DocumentMetrics(req) => &req.path,
                CompilerQueryRequest::ServerInfo(..) => return None,
            })
        }
    }

    #[derive(Debug, Clone)]
    pub enum CompilerQueryResponse {
        OnExport(Option<PathBuf>),
        OnSaveExport(()),
        Hover(Option<Hover>),
        GotoDefinition(Option<GotoDefinitionResponse>),
        GotoDeclaration(Option<GotoDeclarationResponse>),
        References(Option<Vec<LspLocation>>),
        InlayHint(Option<Vec<InlayHint>>),
        CodeLens(Option<Vec<CodeLens>>),
        Completion(Option<CompletionResponse>),
        SignatureHelp(Option<SignatureHelp>),
        PrepareRename(Option<PrepareRenameResponse>),
        Rename(Option<WorkspaceEdit>),
        DocumentSymbol(Option<DocumentSymbolResponse>),
        Symbol(Option<Vec<SymbolInformation>>),
        SemanticTokensFull(Option<SemanticTokensResult>),
        SemanticTokensDelta(Option<SemanticTokensFullDeltaResult>),
        Formatting(Option<Vec<TextEdit>>),
        FoldingRange(Option<Vec<FoldingRange>>),
        SelectionRange(Option<Vec<SelectionRange>>),

        DocumentMetrics(Option<DocumentMetricsResponse>),
        ServerInfo(Option<HashMap<String, ServerInfoReponse>>),
    }
}

pub use polymorphic::*;

#[cfg(test)]
mod tests;
