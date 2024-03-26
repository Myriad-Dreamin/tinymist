pub use std::{
    collections::HashMap,
    iter,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use itertools::{Format, Itertools};
pub use log::{error, trace};
pub use lsp_types::{
    request::GotoDeclarationResponse, CodeLens, CompletionResponse, DiagnosticRelatedInformation,
    DocumentSymbol, DocumentSymbolResponse, Documentation, FoldingRange, GotoDefinitionResponse,
    Hover, InlayHint, Location as LspLocation, LocationLink, MarkupContent, MarkupKind,
    Position as LspPosition, PrepareRenameResponse, SelectionRange, SemanticTokens,
    SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult, SignatureHelp,
    SignatureInformation, SymbolInformation, Url, WorkspaceEdit,
};
pub use reflexo::vector::ir::DefId;
pub use serde_json::Value as JsonValue;
pub use typst::diag::{EcoString, FileError, FileResult, Tracepoint};
pub use typst::foundations::{Func, ParamInfo, Value};
pub use typst::syntax::FileId as TypstFileId;
pub use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, Source, Spanned, SyntaxKind,
};
pub use typst::World;

pub use crate::analysis::{analyze_expr, AnalysisContext};
pub use crate::lsp_typst_boundary::{
    lsp_to_typst, typst_to_lsp, LspDiagnostic, LspRange, LspSeverity, PositionEncoding,
    TypstDiagnostic, TypstSeverity, TypstSpan,
};
pub use crate::VersionedDocument;
