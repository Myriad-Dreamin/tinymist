pub use std::{
    collections::HashMap,
    iter,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use itertools::{Format, Itertools};
pub use log::{error, trace};
pub use tower_lsp::lsp_types::{
    CompletionResponse, DiagnosticRelatedInformation, DocumentSymbol, DocumentSymbolResponse,
    Documentation, FoldingRange, GotoDefinitionResponse, Hover, Location as LspLocation,
    MarkupContent, MarkupKind, Position as LspPosition, SelectionRange, SemanticTokens,
    SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult, SignatureHelp,
    SignatureInformation, SymbolInformation, Url,
};
pub use typst::diag::{EcoString, FileError, FileResult, Tracepoint};
pub use typst::foundations::{Func, ParamInfo, Value};
pub use typst::syntax::{
    ast::{self, AstNode},
    FileId, LinkedNode, Source, Spanned, SyntaxKind, VirtualPath,
};
pub use typst::World;
use typst_ts_compiler::service::WorkspaceProvider;
pub use typst_ts_compiler::TypstSystemWorld;
pub use typst_ts_core::{TypstDocument, TypstFileId};

pub use crate::analysis::analyze_expr;
pub use crate::lsp_typst_boundary::{
    lsp_to_typst, typst_to_lsp, LspDiagnostic, LspRange, LspRawRange, LspSeverity,
    PositionEncoding, TypstDiagnostic, TypstSeverity, TypstSpan,
};

pub fn get_suitable_source_in_workspace(w: &TypstSystemWorld, p: &Path) -> FileResult<Source> {
    // todo: source in packages
    let relative_path = p
        .strip_prefix(&w.workspace_root())
        .map_err(|_| FileError::NotFound(p.to_owned()))?;
    w.source(TypstFileId::new(None, VirtualPath::new(relative_path)))
}
