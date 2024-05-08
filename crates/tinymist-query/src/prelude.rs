pub use std::{
    collections::HashMap,
    iter,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use ecow::EcoVec;
pub use itertools::{Format, Itertools};
pub use log::{error, trace};
pub use lsp_types::{
    request::GotoDeclarationResponse, CodeAction, CodeActionKind, CodeActionOrCommand, CodeLens,
    ColorInformation, ColorPresentation, CompletionResponse, DiagnosticRelatedInformation,
    DocumentSymbol, DocumentSymbolResponse, Documentation, FoldingRange, GotoDefinitionResponse,
    Hover, InlayHint, LanguageString, Location as LspLocation, LocationLink, MarkedString,
    MarkupContent, MarkupKind, Position as LspPosition, PrepareRenameResponse, SelectionRange,
    SemanticTokens, SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult,
    SignatureHelp, SignatureInformation, SymbolInformation, TextEdit, Url, WorkspaceEdit,
};
pub use reflexo::vector::ir::DefId;
pub use serde_json::Value as JsonValue;
pub use typst::diag::{EcoString, FileError, FileResult, Tracepoint};
pub use typst::foundations::{Func, Value};
pub use typst::syntax::FileId as TypstFileId;
pub use typst::syntax::{
    ast::{self, AstNode},
    package::{PackageManifest, PackageSpec},
    LinkedNode, Source, Spanned, SyntaxKind, VirtualPath,
};
pub use typst::World;

pub use crate::analysis::{analyze_expr, AnalysisContext};
pub use crate::lsp_typst_boundary::{
    lsp_to_typst, path_to_url, typst_to_lsp, LspDiagnostic, LspRange, LspSeverity,
    PositionEncoding, TypstDiagnostic, TypstSeverity, TypstSpan,
};
pub use crate::{StatefulRequest, VersionedDocument};
