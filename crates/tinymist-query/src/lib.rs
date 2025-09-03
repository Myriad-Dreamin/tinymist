//! # tinymist-query
//!
//! **Note: this crate is under development. it currently doesn't ensure stable
//! APIs, and heavily depending on some unstable crates.**
//!
//! This crate provides a set of APIs to query the information about the source
//! code. Currently it provides:
//! + language queries defined by the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).

pub use analysis::{CompletionFeat, LocalContext, LocalContextGuard, LspWorldExt};
pub use completion::{CompletionRequest, PostfixSnippet};
pub use typlite::ColorTheme;
pub use upstream::with_vm;

pub use check::*;
pub use code_action::*;
pub use code_context::*;
pub use code_lens::*;
pub use color_presentation::*;
pub use diagnostics::*;
pub use document_color::*;
pub use document_highlight::*;
pub use document_link::*;
pub use document_metrics::*;
pub use document_symbol::*;
pub use folding_range::*;
pub use goto_declaration::*;
pub use goto_definition::*;
pub use hover::*;
pub use inlay_hint::*;
pub use jump::*;
pub use lsp_typst_boundary::*;
pub use on_enter::*;
pub use prepare_rename::*;
pub use references::*;
pub use rename::*;
pub use selection_range::*;
pub use semantic_tokens_delta::*;
pub use semantic_tokens_full::*;
pub use signature_help::*;
pub use symbol::*;
pub use will_rename_files::*;
pub use workspace_label::*;

pub mod analysis;
pub mod docs;
pub mod package;
pub mod syntax;
pub mod testing;
pub use tinymist_analysis::{ty, upstream};

/// The physical position in a document.
pub type FramePosition = typst::layout::Position;

mod adt;
mod lsp_typst_boundary;
mod prelude;

mod bib;
mod check;
mod code_action;
mod code_context;
mod code_lens;
mod color_presentation;
mod completion;
mod diagnostics;
mod document_color;
mod document_highlight;
mod document_link;
mod document_metrics;
mod document_symbol;
mod folding_range;
mod goto_declaration;
mod goto_definition;
mod hover;
mod inlay_hint;
mod jump;
mod on_enter;
mod prepare_rename;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens_delta;
mod semantic_tokens_full;
mod signature_help;
mod symbol;
mod will_rename_files;
mod workspace_label;

use typst::syntax::Source;

use tinymist_analysis::{adt::interner::Interned, log_debug_ct};
use tinymist_project::LspComputeGraph;

/// A reference to the interned string
pub(crate) type StrRef = Interned<str>;

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

/// A request handler with given (semantic) analysis context and a project
/// snapshot.
pub trait StatefulRequest {
    /// The response type of the request.
    type Response;

    /// Request the information from the given context.
    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response>;
}

mod polymorphic {
    use completion::CompletionList;
    use lsp_types::TextEdit;
    use serde::{Deserialize, Serialize};
    use tinymist_project::ProjectTask;
    use typst::foundations::Dict;

    use super::prelude::*;
    use super::*;

    /// A request to run an export task.
    #[derive(Debug, Clone)]
    pub struct OnExportRequest {
        /// The path of the document to export.
        pub path: PathBuf,
        /// The export task to run.
        pub task: ProjectTask,
        /// Whether to open the exported file(s) after the export is done.
        pub open: bool,
        /// Whether to write to file.
        pub write: bool,
    }

    /// The response to an export request.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum OnExportResponse {
        Failed {
            message: String,
        },
        Single {
            path: Option<PathBuf>,
            data: Option<String>,
        },
        Multiple(Vec<PagedExportResponse>),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PagedExportResponse {
        pub page: usize,
        pub path: Option<PathBuf>,
        pub data: Option<String>,
    }

    /// A request to format the document.
    #[derive(Debug, Clone)]
    pub struct FormattingRequest {
        /// The path of the document to get semantic tokens for.
        pub path: PathBuf,
    }

    /// A request to get the server info.
    #[derive(Debug, Clone)]
    pub struct ServerInfoRequest {}

    /// The response to the server info request.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ServerInfoResponse {
        /// The root path of the server.
        pub root: Option<PathBuf>,
        /// The font paths of the server.
        pub font_paths: Vec<PathBuf>,
        /// The inputs of the server.
        pub inputs: Dict,
        /// The statistics of the server.
        pub stats: HashMap<String, String>,
    }

    /// The feature of the fold request.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FoldRequestFeature {
        /// Serves the request with the first pinned entry.
        PinnedFirst,
        /// Makes the items unique.
        Unique,
        /// Merges the items.
        Mergeable,
        /// Makes the items unique without context.
        ContextFreeUnique,
    }

    /// The analysis request.
    #[derive(Debug, Clone, strum::IntoStaticStr)]
    pub enum CompilerQueryRequest {
        /// A request to run an export task.
        OnExport(OnExportRequest),
        /// A request to get the hover information.
        Hover(HoverRequest),
        /// A request to go to the definition.
        GotoDefinition(GotoDefinitionRequest),
        /// A request to go to the declaration.
        GotoDeclaration(GotoDeclarationRequest),
        /// A request to get the references.
        References(ReferencesRequest),
        /// A request to get the inlay hints.
        InlayHint(InlayHintRequest),
        /// A request to get the document colors.
        DocumentColor(DocumentColorRequest),
        /// A request to get the document links.
        DocumentLink(DocumentLinkRequest),
        /// A request to get the document highlights.
        DocumentHighlight(DocumentHighlightRequest),
        /// A request to get the color presentations.
        ColorPresentation(ColorPresentationRequest),
        /// A request to get the code actions.
        CodeAction(CodeActionRequest),
        /// A request to get the code lenses.
        CodeLens(CodeLensRequest),
        /// A request to get the completions.
        Completion(CompletionRequest),
        /// A request to get the signature helps.
        SignatureHelp(SignatureHelpRequest),
        /// A request to rename.
        Rename(RenameRequest),
        /// A request to determine the files to be renamed.
        WillRenameFiles(WillRenameFilesRequest),
        /// A request to prepare the rename.
        PrepareRename(PrepareRenameRequest),
        /// A request to get the document symbols.
        DocumentSymbol(DocumentSymbolRequest),
        /// A request to get the symbols.
        Symbol(SymbolRequest),
        /// A request to get the semantic tokens full.
        SemanticTokensFull(SemanticTokensFullRequest),
        /// A request to get the semantic tokens delta.
        SemanticTokensDelta(SemanticTokensDeltaRequest),
        /// A request to format the document.
        Formatting(FormattingRequest),
        /// A request to get the folding ranges.
        FoldingRange(FoldingRangeRequest),
        /// A request to get the selection ranges.
        SelectionRange(SelectionRangeRequest),
        /// A request to interact with the code context.
        InteractCodeContext(InteractCodeContextRequest),

        /// A request to get extra text edits on enter.
        OnEnter(OnEnterRequest),

        /// A request to get the document metrics.
        DocumentMetrics(DocumentMetricsRequest),
        /// A request to get the workspace labels.
        WorkspaceLabel(WorkspaceLabelRequest),
        /// A request to get the server info.
        ServerInfo(ServerInfoRequest),
    }

    impl CompilerQueryRequest {
        /// Gets the feature of the fold request.
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

        /// Gets the associated path of the request.
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

    /// The response to the compiler query request.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum CompilerQueryResponse {
        /// The response to the on export request.
        OnExport(Option<OnExportResponse>),
        /// The response to the hover request.
        Hover(Option<Hover>),
        /// The response to the goto definition request.
        GotoDefinition(Option<GotoDefinitionResponse>),
        /// The response to the goto declaration request.
        GotoDeclaration(Option<GotoDeclarationResponse>),
        /// The response to the references request.
        References(Option<Vec<LspLocation>>),
        /// The response to the inlay hint request.
        InlayHint(Option<Vec<InlayHint>>),
        /// The response to the document color request.
        DocumentColor(Option<Vec<ColorInformation>>),
        /// The response to the document link request.
        DocumentLink(Option<Vec<DocumentLink>>),
        /// The response to the document highlight request.
        DocumentHighlight(Option<Vec<DocumentHighlight>>),
        /// The response to the color presentation request.
        ColorPresentation(Option<Vec<ColorPresentation>>),
        /// The response to the code action request.
        CodeAction(Option<Vec<CodeAction>>),
        /// The response to the code lens request.
        CodeLens(Option<Vec<CodeLens>>),
        /// The response to the completion request.
        Completion(Option<CompletionList>),
        /// The response to the signature help request.
        SignatureHelp(Option<SignatureHelp>),
        /// The response to the prepare rename request.
        PrepareRename(Option<PrepareRenameResponse>),
        /// The response to the rename request.
        Rename(Option<WorkspaceEdit>),
        /// The response to the will rename files request.
        WillRenameFiles(Option<WorkspaceEdit>),
        /// The response to the document symbol request.
        DocumentSymbol(Option<DocumentSymbolResponse>),
        /// The response to the symbol request.
        Symbol(Option<Vec<SymbolInformation>>),
        /// The response to the workspace label request.
        WorkspaceLabel(Option<Vec<SymbolInformation>>),
        /// The response to the semantic tokens full request.
        SemanticTokensFull(Option<SemanticTokensResult>),
        /// The response to the semantic tokens delta request.
        SemanticTokensDelta(Option<SemanticTokensFullDeltaResult>),
        /// The response to the formatting request.
        Formatting(Option<Vec<TextEdit>>),
        /// The response to the folding range request.
        FoldingRange(Option<Vec<FoldingRange>>),
        /// The response to the selection range request.
        SelectionRange(Option<Vec<SelectionRange>>),
        /// The response to the interact code context request.
        InteractCodeContext(Option<Vec<Option<InteractCodeContextResponse>>>),

        /// The response to the on enter request.
        OnEnter(Option<Vec<TextEdit>>),

        /// The response to the document metrics request.
        DocumentMetrics(Option<DocumentMetricsResponse>),
        /// The response to the server info request.
        ServerInfo(Option<HashMap<String, ServerInfoResponse>>),
    }
}

pub use polymorphic::*;

#[cfg(test)]
mod tests;
