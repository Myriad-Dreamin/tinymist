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
pub mod docs;
pub mod package;
pub mod syntax;
pub mod testing;
pub mod ty;
mod upstream;

pub use analysis::{CompletionFeat, LocalContext, LocalContextGuard, LspWorldExt};
pub use completion::PostfixSnippet;
pub use upstream::with_vm;

mod diagnostics;
pub use diagnostics::*;
mod code_action;
pub use code_action::*;
mod code_context;
pub use code_context::*;
mod code_lens;
pub use code_lens::*;
mod completion;
pub use completion::CompletionRequest;
mod color_presentation;
pub use color_presentation::*;
mod document_color;
pub use document_color::*;
mod document_highlight;
pub use document_highlight::*;
mod document_symbol;
pub use document_symbol::*;
mod document_link;
pub use document_link::*;
mod workspace_label;
pub use workspace_label::*;
mod document_metrics;
pub use document_metrics::*;
mod folding_range;
pub use folding_range::*;
mod goto_declaration;
pub use goto_declaration::*;
mod goto_definition;
pub use goto_definition::*;
mod hover;
pub use hover::*;
mod inlay_hint;
pub use inlay_hint::*;
mod jump;
pub use jump::*;
mod will_rename_files;
pub use will_rename_files::*;
mod rename;
pub use rename::*;
mod selection_range;
pub use selection_range::*;
mod semantic_tokens_full;
pub use semantic_tokens_full::*;
mod semantic_tokens_delta;
pub use semantic_tokens_delta::*;
mod signature_help;
pub use signature_help::*;
mod symbol;
pub use symbol::*;
mod on_enter;
pub use on_enter::*;
mod prepare_rename;
pub use prepare_rename::*;
mod references;
pub use references::*;

mod lsp_typst_boundary;
pub use lsp_typst_boundary::*;

mod prelude;

use tinymist_std::typst::TypstDocument;
use typst::syntax::Source;

/// The physical position in a document.
pub type FramePosition = typst::layout::Position;

pub use typlite::ColorTheme;

/// A compiled document with an self-incremented logical version.
#[derive(Debug, Clone)]
pub struct VersionedDocument {
    /// The version of the document.
    pub version: usize,
    /// The compiled document.
    pub document: TypstDocument,
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
    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response>;
}

/// A request handler with given (semantic) analysis context and a versioned
/// document.
pub trait StatefulRequest {
    /// The response type of the request.
    type Response;

    /// Request the information from the given context.
    fn request(
        self,
        ctx: &mut LocalContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response>;
}

use tinymist_analysis::log_debug_ct;

#[allow(missing_docs)]
mod polymorphic {
    use completion::CompletionList;
    use lsp_types::TextEdit;
    use serde::{Deserialize, Serialize};
    use tinymist_project::ProjectTask;
    use typst::foundations::Dict;

    use super::prelude::*;
    use super::*;

    #[derive(Debug, Clone)]
    pub struct OnExportRequest {
        /// The path of the document to export.
        pub path: PathBuf,
        /// The export task to run.
        pub task: ProjectTask,
        /// Whether to open the exported file(s) after the export is done.
        pub open: bool,
    }

    #[derive(Debug, Clone)]
    pub struct FormattingRequest {
        /// The path of the document to get semantic tokens for.
        pub path: PathBuf,
    }

    #[derive(Debug, Clone)]
    pub struct ServerInfoRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ServerInfoResponse {
        pub root: Option<PathBuf>,
        pub font_paths: Vec<PathBuf>,
        pub inputs: Dict,
        pub stats: HashMap<String, String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FoldRequestFeature {
        PinnedFirst,
        Unique,
        Mergeable,
        ContextFreeUnique,
    }

    #[derive(Debug, Clone, strum::IntoStaticStr)]
    pub enum CompilerQueryRequest {
        OnExport(OnExportRequest),
        Hover(HoverRequest),
        GotoDefinition(GotoDefinitionRequest),
        GotoDeclaration(GotoDeclarationRequest),
        References(ReferencesRequest),
        InlayHint(InlayHintRequest),
        DocumentColor(DocumentColorRequest),
        DocumentLink(DocumentLinkRequest),
        DocumentHighlight(DocumentHighlightRequest),
        ColorPresentation(ColorPresentationRequest),
        CodeAction(CodeActionRequest),
        CodeLens(CodeLensRequest),
        Completion(CompletionRequest),
        SignatureHelp(SignatureHelpRequest),
        Rename(RenameRequest),
        WillRenameFiles(WillRenameFilesRequest),
        PrepareRename(PrepareRenameRequest),
        DocumentSymbol(DocumentSymbolRequest),
        Symbol(SymbolRequest),
        SemanticTokensFull(SemanticTokensFullRequest),
        SemanticTokensDelta(SemanticTokensDeltaRequest),
        Formatting(FormattingRequest),
        FoldingRange(FoldingRangeRequest),
        SelectionRange(SelectionRangeRequest),
        InteractCodeContext(InteractCodeContextRequest),

        OnEnter(OnEnterRequest),

        DocumentMetrics(DocumentMetricsRequest),
        WorkspaceLabel(WorkspaceLabelRequest),
        ServerInfo(ServerInfoRequest),
    }

    impl CompilerQueryRequest {
        pub fn fold_feature(&self) -> FoldRequestFeature {
            use FoldRequestFeature::*;
            match self {
                Self::OnExport(..) => Mergeable,
                Self::Hover(..) => PinnedFirst,
                Self::GotoDefinition(..) => PinnedFirst,
                Self::GotoDeclaration(..) => PinnedFirst,
                Self::References(..) => PinnedFirst,
                Self::InlayHint(..) => Unique,
                Self::DocumentColor(..) => PinnedFirst,
                Self::DocumentLink(..) => PinnedFirst,
                Self::DocumentHighlight(..) => PinnedFirst,
                Self::ColorPresentation(..) => ContextFreeUnique,
                Self::CodeAction(..) => Unique,
                Self::CodeLens(..) => Unique,
                Self::Completion(..) => Mergeable,
                Self::SignatureHelp(..) => PinnedFirst,
                Self::Rename(..) => Mergeable,
                Self::WillRenameFiles(..) => Mergeable,
                Self::PrepareRename(..) => Mergeable,
                Self::DocumentSymbol(..) => ContextFreeUnique,
                Self::WorkspaceLabel(..) => Mergeable,
                Self::Symbol(..) => Mergeable,
                Self::SemanticTokensFull(..) => PinnedFirst,
                Self::SemanticTokensDelta(..) => PinnedFirst,
                Self::Formatting(..) => ContextFreeUnique,
                Self::FoldingRange(..) => ContextFreeUnique,
                Self::SelectionRange(..) => ContextFreeUnique,
                Self::InteractCodeContext(..) => PinnedFirst,

                Self::OnEnter(..) => ContextFreeUnique,

                Self::DocumentMetrics(..) => PinnedFirst,
                Self::ServerInfo(..) => Mergeable,
            }
        }

        pub fn associated_path(&self) -> Option<&Path> {
            Some(match self {
                Self::OnExport(..) => return None,
                Self::Hover(req) => &req.path,
                Self::GotoDefinition(req) => &req.path,
                Self::GotoDeclaration(req) => &req.path,
                Self::References(req) => &req.path,
                Self::InlayHint(req) => &req.path,
                Self::DocumentColor(req) => &req.path,
                Self::DocumentLink(req) => &req.path,
                Self::DocumentHighlight(req) => &req.path,
                Self::ColorPresentation(req) => &req.path,
                Self::CodeAction(req) => &req.path,
                Self::CodeLens(req) => &req.path,
                Self::Completion(req) => &req.path,
                Self::SignatureHelp(req) => &req.path,
                Self::Rename(req) => &req.path,
                Self::WillRenameFiles(..) => return None,
                Self::PrepareRename(req) => &req.path,
                Self::DocumentSymbol(req) => &req.path,
                Self::Symbol(..) => return None,
                Self::WorkspaceLabel(..) => return None,
                Self::SemanticTokensFull(req) => &req.path,
                Self::SemanticTokensDelta(req) => &req.path,
                Self::Formatting(req) => &req.path,
                Self::FoldingRange(req) => &req.path,
                Self::SelectionRange(req) => &req.path,
                Self::InteractCodeContext(req) => &req.path,

                Self::OnEnter(req) => &req.path,

                Self::DocumentMetrics(req) => &req.path,
                Self::ServerInfo(..) => return None,
            })
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CompilerQueryResponse {
        OnExport(Option<PathBuf>),
        Hover(Option<Hover>),
        GotoDefinition(Option<GotoDefinitionResponse>),
        GotoDeclaration(Option<GotoDeclarationResponse>),
        References(Option<Vec<LspLocation>>),
        InlayHint(Option<Vec<InlayHint>>),
        DocumentColor(Option<Vec<ColorInformation>>),
        DocumentLink(Option<Vec<DocumentLink>>),
        DocumentHighlight(Option<Vec<DocumentHighlight>>),
        ColorPresentation(Option<Vec<ColorPresentation>>),
        CodeAction(Option<Vec<CodeActionOrCommand>>),
        CodeLens(Option<Vec<CodeLens>>),
        Completion(Option<CompletionList>),
        SignatureHelp(Option<SignatureHelp>),
        PrepareRename(Option<PrepareRenameResponse>),
        Rename(Option<WorkspaceEdit>),
        WillRenameFiles(Option<WorkspaceEdit>),
        DocumentSymbol(Option<DocumentSymbolResponse>),
        Symbol(Option<Vec<SymbolInformation>>),
        WorkspaceLabel(Option<Vec<SymbolInformation>>),
        SemanticTokensFull(Option<SemanticTokensResult>),
        SemanticTokensDelta(Option<SemanticTokensFullDeltaResult>),
        Formatting(Option<Vec<TextEdit>>),
        FoldingRange(Option<Vec<FoldingRange>>),
        SelectionRange(Option<Vec<SelectionRange>>),
        InteractCodeContext(Option<Vec<Option<InteractCodeContextResponse>>>),

        OnEnter(Option<Vec<TextEdit>>),

        DocumentMetrics(Option<DocumentMetricsResponse>),
        ServerInfo(Option<HashMap<String, ServerInfoResponse>>),
    }
}

pub use polymorphic::*;

#[cfg(test)]
mod tests;
