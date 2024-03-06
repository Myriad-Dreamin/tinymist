use std::ffi::OsStr;
use std::path::PathBuf;

use tower_lsp::lsp_types::{DocumentFormattingClientCapabilities, Url};
use tower_lsp::lsp_types::{
    InitializeParams, Position, PositionEncodingKind, SemanticTokensClientCapabilities,
};
use typst::syntax::VirtualPath;

use crate::config::PositionEncoding;

pub trait InitializeParamsExt {
    fn position_encodings(&self) -> &[PositionEncodingKind];
    fn supports_config_change_registration(&self) -> bool;
    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities>;
    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities>;
    fn supports_semantic_tokens_dynamic_registration(&self) -> bool;
    fn supports_document_formatting_dynamic_registration(&self) -> bool;
    fn root_paths(&self) -> Vec<PathBuf>;
}

static DEFAULT_ENCODING: [PositionEncodingKind; 1] = [PositionEncodingKind::UTF16];

impl InitializeParamsExt for InitializeParams {
    fn position_encodings(&self) -> &[PositionEncodingKind] {
        self.capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .map(|encodings| encodings.as_slice())
            .unwrap_or(&DEFAULT_ENCODING)
    }

    fn supports_config_change_registration(&self) -> bool {
        self.capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.configuration)
            .unwrap_or(false)
    }

    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .semantic_tokens
            .as_ref()
    }

    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .formatting
            .as_ref()
    }

    fn supports_semantic_tokens_dynamic_registration(&self) -> bool {
        self.semantic_tokens_capabilities()
            .and_then(|semantic_tokens| semantic_tokens.dynamic_registration)
            .unwrap_or(false)
    }

    fn supports_document_formatting_dynamic_registration(&self) -> bool {
        self.document_formatting_capabilities()
            .and_then(|document_format| document_format.dynamic_registration)
            .unwrap_or(false)
    }

    #[allow(deprecated)] // `self.root_path` is marked as deprecated
    fn root_paths(&self) -> Vec<PathBuf> {
        match self.workspace_folders.as_ref() {
            Some(roots) => roots
                .iter()
                .map(|root| &root.uri)
                .map(Url::to_file_path)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            None => self
                .root_uri
                .as_ref()
                .map(|uri| uri.to_file_path().unwrap())
                .or_else(|| self.root_path.clone().map(PathBuf::from))
                .into_iter()
                .collect(),
        }
    }
}

pub trait StrExt {
    fn encoded_len(&self, encoding: PositionEncoding) -> usize;
}

impl StrExt for str {
    fn encoded_len(&self, encoding: PositionEncoding) -> usize {
        match encoding {
            PositionEncoding::Utf8 => self.len(),
            PositionEncoding::Utf16 => self.chars().map(char::len_utf16).sum(),
        }
    }
}

pub trait VirtualPathExt {
    fn with_extension(&self, extension: impl AsRef<OsStr>) -> Self;
}

impl VirtualPathExt for VirtualPath {
    fn with_extension(&self, extension: impl AsRef<OsStr>) -> Self {
        Self::new(self.as_rooted_path().with_extension(extension))
    }
}

pub trait PositionExt {
    fn delta(&self, to: &Self) -> PositionDelta;
}

impl PositionExt for Position {
    /// Calculates the delta from `self` to `to`. This is in the `SemanticToken`
    /// sense, so the delta's `character` is relative to `self`'s
    /// `character` iff `self` and `to` are on the same line. Otherwise,
    /// it's relative to the start of the line `to` is on.
    fn delta(&self, to: &Self) -> PositionDelta {
        let line_delta = to.line - self.line;
        let char_delta = if line_delta == 0 {
            to.character - self.character
        } else {
            to.character
        };

        PositionDelta {
            delta_line: line_delta,
            delta_start: char_delta,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default)]
pub struct PositionDelta {
    pub delta_line: u32,
    pub delta_start: u32,
}
