pub use std::collections::HashMap;
pub use std::iter;
pub use std::ops::Range;
pub use std::path::{Path, PathBuf};
pub use std::sync::{Arc, LazyLock, OnceLock};

pub use ecow::{eco_vec, EcoVec};
pub use itertools::{Format, Itertools};
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
    lsp_to_typst, path_to_url, typst_to_lsp, LspDiagnostic, LspRange, LspSeverity,
    PositionEncoding, TypstDiagnostic, TypstSeverity, TypstSpan,
};
pub use crate::syntax::{classify_syntax, Decl, DefKind};
pub(crate) use crate::ty::PathPreference;
pub use crate::{SemanticRequest, StatefulRequest, VersionedDocument};
