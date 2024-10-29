pub use std::{
    collections::HashMap,
    iter,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use ecow::eco_vec;
pub use ecow::EcoVec;
pub use itertools::{Format, Itertools};
pub use log::error;
pub use lsp_types::{
    request::GotoDeclarationResponse, CodeAction, CodeActionKind, CodeActionOrCommand, CodeLens,
    ColorInformation, ColorPresentation, CompletionResponse, DiagnosticRelatedInformation,
    DocumentHighlight, DocumentLink, DocumentSymbol, DocumentSymbolResponse, Documentation,
    FoldingRange, GotoDefinitionResponse, Hover, HoverContents, InlayHint, LanguageString,
    Location as LspLocation, LocationLink, MarkedString, MarkupContent, MarkupKind,
    Position as LspPosition, PrepareRenameResponse, SelectionRange, SemanticTokens,
    SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult, SignatureHelp,
    SignatureInformation, SymbolInformation, TextEdit, Url, WorkspaceEdit,
};
pub use reflexo::vector::ir::DefId;
pub use serde_json::Value as JsonValue;
pub use typst::diag::{EcoString, FileResult, Tracepoint};
pub use typst::foundations::Value;
pub use typst::syntax::FileId as TypstFileId;
pub use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, Source, Spanned, SyntaxKind, SyntaxNode,
};
pub use typst::World;

pub use crate::analysis::{AnalysisContext, LocalContext};
pub use crate::lsp_typst_boundary::{
    lsp_to_typst, path_to_url, typst_to_lsp, LspDiagnostic, LspRange, LspSeverity,
    PositionEncoding, TypstDiagnostic, TypstSeverity, TypstSpan,
};
pub use crate::{SemanticRequest, StatefulRequest, VersionedDocument};
