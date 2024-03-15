mod adt;
pub mod analysis;
pub mod syntax;

pub(crate) mod diagnostics;

use std::sync::Arc;

pub use analysis::AnalysisContext;
use typst_ts_core::TypstDocument;

pub use diagnostics::*;
pub(crate) mod code_lens;
pub use code_lens::*;
pub(crate) mod completion;
pub use completion::*;
pub(crate) mod document_symbol;
pub use document_symbol::*;
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

mod prelude;

#[derive(Debug, Clone)]
pub struct VersionedDocument {
    pub version: usize,
    pub document: Arc<TypstDocument>,
}

pub trait SyntaxRequest {
    type Response;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response>;
}

mod polymorphic {
    use super::prelude::*;
    use super::*;

    #[derive(Debug, Clone)]
    pub struct OnExportRequest {
        pub path: PathBuf,
    }

    #[derive(Debug, Clone)]
    pub struct OnSaveExportRequest {
        pub path: PathBuf,
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
        FoldingRange(FoldingRangeRequest),
        SelectionRange(SelectionRangeRequest),
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
                CompilerQueryRequest::FoldingRange(..) => ContextFreeUnique,
                CompilerQueryRequest::SelectionRange(..) => ContextFreeUnique,
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
                CompilerQueryRequest::FoldingRange(req) => &req.path,
                CompilerQueryRequest::SelectionRange(req) => &req.path,
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
        FoldingRange(Option<Vec<FoldingRange>>),
        SelectionRange(Option<Vec<SelectionRange>>),
    }
}

pub use polymorphic::*;

#[cfg(test)]
mod tests;
