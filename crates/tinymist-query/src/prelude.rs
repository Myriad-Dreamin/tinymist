pub use std::{
    collections::HashMap,
    iter,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use comemo::{Track, Tracked};
pub use itertools::{Format, Itertools};
pub use log::{error, trace};
pub use lsp_types::{
    CodeLens, CompletionResponse, DiagnosticRelatedInformation, DocumentSymbol,
    DocumentSymbolResponse, Documentation, FoldingRange, GotoDefinitionResponse, Hover, InlayHint,
    Location as LspLocation, MarkupContent, MarkupKind, Position as LspPosition,
    PrepareRenameResponse, SelectionRange, SemanticTokens, SemanticTokensDelta,
    SemanticTokensFullDeltaResult, SemanticTokensResult, SignatureHelp, SignatureInformation,
    SymbolInformation, Url, WorkspaceEdit,
};
pub use serde_json::Value as JsonValue;
pub use typst::diag::{EcoString, FileError, FileResult, Tracepoint};
pub use typst::foundations::{Func, ParamInfo, Value};
pub use typst::syntax::{
    ast::{self, AstNode},
    FileId, LinkedNode, Source, Spanned, SyntaxKind, VirtualPath,
};
pub use typst::World;
use typst_ts_compiler::service::WorkspaceProvider;
pub use typst_ts_compiler::TypstSystemWorld;
pub use typst_ts_core::TypstFileId;

pub use crate::analysis::analyze_expr;
pub use crate::lsp_typst_boundary::{
    lsp_to_typst, typst_to_lsp, LspDiagnostic, LspRange, LspSeverity, PositionEncoding,
    TypstDiagnostic, TypstSeverity, TypstSpan,
};
pub use crate::VersionedDocument;

pub fn get_suitable_source_in_workspace(w: &TypstSystemWorld, p: &Path) -> FileResult<Source> {
    // todo: source in packages
    let relative_path = p
        .strip_prefix(&w.workspace_root())
        .map_err(|_| FileError::NotFound(p.to_owned()))?;
    w.source(TypstFileId::new(None, VirtualPath::new(relative_path)))
}
