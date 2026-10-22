pub use std::collections::HashMap;
pub use std::iter;
pub use std::ops::Range;
pub use std::path::{Path, PathBuf};
pub use std::sync::{Arc, LazyLock, OnceLock};

pub use ecow::{EcoVec, eco_vec};
pub use itertools::Itertools;
pub use lsp_types::{
    ActiveParameter, BaseSymbolInformation, CodeActionKind, CodeLens, ColorInformation,
    ColorPresentation, Contents, DeclarationResponse, DefinitionResponse, Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DocumentChange, DocumentHighlight,
    DocumentLink, DocumentSymbol, DocumentSymbolResponse, Documentation, FoldingRange, Hover,
    InlayHint, Location as LspLocation, LocationLink, MarkupContent, MarkupKind,
    ParameterInformation, Position as LspPosition, PrepareRenamePlaceholder, PrepareRenameResult,
    SelectionRange, SemanticTokens, SemanticTokensDelta, SemanticTokensDeltaResponse,
    SignatureHelp, SignatureInformation, SymbolInformation, TextDocumentIdentifier, TextEdit,
    Uri as Url, WorkspaceEdit,
};
pub use serde_json::Value as JsonValue;
pub use tinymist_project::LspComputeGraph;
pub use tinymist_std::DefId;
pub use typst::World;
pub use typst::diag::{EcoString, Tracepoint};
pub use typst::foundations::Value;
pub use typst::syntax::ast::{self, AstNode};
pub use typst::syntax::{
    DiagSpan, FileId as TypstFileId, LinkedNode, Source, Spanned, SyntaxKind, SyntaxNode,
};
pub use typst_shim::syntax::{
    LinkedNodeExt, RootedPathExt, VirtualPathExt, resolve_path_from_id, source_range,
};

pub use crate::SemanticRequest;
pub use crate::analysis::{Definition, LocalContext};
pub use crate::code_action::proto::*;
pub use crate::docs::DefDocs;
pub use crate::lsp_typst_boundary::{
    LspRange, PositionEncoding, path_to_url, to_lsp_position, to_lsp_range, to_typst_position,
    to_typst_range,
};
pub use crate::syntax::{Decl, DefKind, classify_syntax};
pub(crate) use crate::ty::PathKind;
