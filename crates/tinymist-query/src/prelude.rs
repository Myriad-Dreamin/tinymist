pub use std::collections::HashMap;
pub use std::iter;
pub use std::ops::Range;
pub use std::path::{Path, PathBuf};
pub use std::sync::{Arc, LazyLock, OnceLock};

pub use ecow::{eco_vec, EcoVec};
pub use itertools::{Format, Itertools};
pub use lsp_types::{
    request::GotoDeclarationResponse, CodeAction, CodeActionKind, CodeActionOrCommand, CodeLens,
    ColorInformation, ColorPresentation, Diagnostic, DiagnosticRelatedInformation,
    DiagnosticSeverity, DocumentHighlight, DocumentLink, DocumentSymbol, DocumentSymbolResponse,
    Documentation, FoldingRange, GotoDefinitionResponse, Hover, HoverContents, InlayHint,
    Location as LspLocation, LocationLink, MarkedString, MarkupContent, MarkupKind,
    ParameterInformation, Position as LspPosition, PrepareRenameResponse, SelectionRange,
    SemanticTokens, SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult,
    SignatureHelp, SignatureInformation, SymbolInformation, TextEdit, Url, WorkspaceEdit,
};
pub use serde_json::Value as JsonValue;
pub use tinymist_project::LspCompileSnapshot;
pub use tinymist_std::DefId;
pub use typst::diag::{EcoString, Tracepoint};
pub use typst::foundations::Value;
pub use typst::syntax::ast::{self, AstNode};
pub use typst::syntax::{
    FileId as TypstFileId, LinkedNode, Source, Spanned, SyntaxKind, SyntaxNode,
};
pub use typst::World;
pub use typst_shim::syntax::LinkedNodeExt;

pub use crate::analysis::{Definition, LocalContext};
pub use crate::docs::DefDocs;
pub use crate::lsp_typst_boundary::{
    path_to_url, to_lsp_position, to_lsp_range, to_typst_position, to_typst_range, LspRange,
    PositionEncoding,
};
pub use crate::syntax::{classify_syntax, Decl, DefKind};
pub(crate) use crate::ty::PathPreference;
pub use crate::{SemanticRequest, StatefulRequest};
