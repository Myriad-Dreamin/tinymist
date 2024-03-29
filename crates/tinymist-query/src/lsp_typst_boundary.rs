//! Conversions between Typst and LSP types and representations

// todo: remove this
#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use lsp_types::{self, Url};
use once_cell::sync::Lazy;
use reflexo::path::PathClean;

pub type LspPosition = lsp_types::Position;
/// The interpretation of an `LspCharacterOffset` depends on the
/// `LspPositionEncoding`
pub type LspCharacterOffset = u32;
pub type LspPositionEncoding = PositionEncoding;
/// Byte offset (i.e. UTF-8 bytes) in Typst files, either from the start of the
/// line or the file
pub type TypstOffset = usize;
pub type TypstSpan = typst::syntax::Span;

/// An LSP range. It needs its associated `LspPositionEncoding` to be used. The
/// `LspRange` struct provides this range with that encoding.
pub type LspRange = lsp_types::Range;
pub type TypstRange = std::ops::Range<usize>;

pub type TypstTooltip = crate::upstream::Tooltip;
pub type LspHoverContents = lsp_types::HoverContents;

pub type LspDiagnostic = lsp_types::Diagnostic;
pub type TypstDiagnostic = typst::diag::SourceDiagnostic;

pub type LspSeverity = lsp_types::DiagnosticSeverity;
pub type TypstSeverity = typst::diag::Severity;

pub type LspParamInfo = lsp_types::ParameterInformation;
pub type TypstParamInfo = typst::foundations::ParamInfo;

/// What counts as "1 character" for string indexing. We should always prefer
/// UTF-8, but support UTF-16 as long as it is standard. For more background on
/// encodings and LSP, try ["The bottom emoji breaks rust-analyzer"](https://fasterthanli.me/articles/the-bottom-emoji-breaks-rust-analyzer),
/// a well-written article on the topic.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub enum PositionEncoding {
    /// "1 character" means "1 UTF-16 code unit"
    ///
    /// This is the only required encoding for LSPs to support, but it's not a
    /// natural one (unless you're working in JS). Prefer UTF-8, and refer
    /// to the article linked in the `PositionEncoding` docs for more
    /// background.
    #[default]
    Utf16,
    /// "1 character" means "1 byte"
    Utf8,
}

impl From<PositionEncoding> for lsp_types::PositionEncodingKind {
    fn from(position_encoding: PositionEncoding) -> Self {
        match position_encoding {
            PositionEncoding::Utf16 => Self::UTF16,
            PositionEncoding::Utf8 => Self::UTF8,
        }
    }
}

pub type LspCompletion = lsp_types::CompletionItem;
pub type LspCompletionKind = lsp_types::CompletionItemKind;
pub type TypstCompletion = crate::upstream::Completion;
pub type TypstCompletionKind = crate::upstream::CompletionKind;

const UNTITLED_ROOT: &str = "/untitled";
static EMPTY_URL: Lazy<Url> = Lazy::new(|| Url::parse("file://").unwrap());

pub fn path_to_url(path: &Path) -> anyhow::Result<Url> {
    if let Ok(untitled) = path.strip_prefix(UNTITLED_ROOT) {
        // rust-url will panic on converting an empty path.
        if untitled == Path::new("neovim-bug") {
            return Ok(EMPTY_URL.clone());
        }

        return Ok(Url::parse(&format!("untitled:{}", untitled.display()))?);
    }

    Url::from_file_path(path).or_else(|e| {
        let _: () = e;

        anyhow::bail!("could not convert path to URI: path: {path:?}",)
    })
}

pub fn url_to_path(uri: Url) -> PathBuf {
    if uri.scheme() == "file" {
        return uri.to_file_path().unwrap_or_else(|_| {
            // typst converts an empty path to `Path::new("/")`, which is undesirable.
            if !uri.has_host() && uri.path() == "/" {
                return PathBuf::from("/untitled/neovim-bug");
            }

            panic!("could not convert URI to path: URI: {uri:?}",)
        });
    }

    if uri.scheme() == "untitled" {
        let mut bytes = UNTITLED_ROOT.as_bytes().to_vec();

        // This is rust-url's path_segments, but vscode's untitle doesn't like it.
        let path = uri.path();
        let segs = path.strip_prefix('/').unwrap_or(path).split('/');
        for segment in segs {
            bytes.push(b'/');
            bytes.extend(percent_encoding::percent_decode(segment.as_bytes()));
        }

        return Path::new(String::from_utf8_lossy(&bytes).as_ref()).clean();
    }

    uri.to_file_path().unwrap()
}

pub mod lsp_to_typst {
    use typst::syntax::Source;

    use super::*;

    pub fn position(
        lsp_position: LspPosition,
        lsp_position_encoding: LspPositionEncoding,
        typst_source: &Source,
    ) -> Option<TypstOffset> {
        let lines = typst_source.len_lines() as u32;
        if lsp_position.line >= lines
            || (lsp_position.line + 1 == lines && {
                let last_line_offset = typst_source.line_to_byte(lines as usize - 1)?;
                let last_line_chars = &typst_source.text()[last_line_offset..];
                let len = match lsp_position_encoding {
                    LspPositionEncoding::Utf8 => last_line_chars.len(),
                    LspPositionEncoding::Utf16 => {
                        last_line_chars.chars().map(char::len_utf16).sum::<usize>()
                    }
                };
                lsp_position.character as usize >= len
            })
        {
            if lsp_position.line > lines || lsp_position.character > 0 {
                log::warn!(
                    "LSP position is out of bounds: {:?}, while only {:?} lines and {:?} characters at the end.",
                    lsp_position, typst_source.len_lines(), typst_source.line_to_range(typst_source.len_lines() - 1),
                );
            }

            return Some(typst_source.len_bytes());
        }

        match lsp_position_encoding {
            LspPositionEncoding::Utf8 => {
                let line_index = lsp_position.line as usize;
                let column_index = lsp_position.character as usize;
                typst_source.line_column_to_byte(line_index, column_index)
            }
            LspPositionEncoding::Utf16 => {
                // We have a line number and a UTF-16 offset into that line. We want a byte
                // offset into the file.
                //
                // Typst's `Source` provides several UTF-16 methods:
                //  - `len_utf16` for the length of the file
                //  - `byte_to_utf16` to convert a byte offset from the start of the file to a
                //    UTF-16 offset from the start of the file
                //  - `utf16_to_byte` to do the opposite of `byte_to_utf16`
                //
                // Unfortunately, none of these address our needs well, so we do some math
                // instead. This is not the fastest possible implementation, but
                // it's the most reasonable without access to the internal state
                // of `Source`.

                // TODO: Typst's `Source` could easily provide an implementation of the method
                // we need   here. Submit a PR against `typst` to add it, then
                // update this if/when merged.

                let line_index = lsp_position.line as usize;
                let utf16_offset_in_line = lsp_position.character as usize;

                let byte_line_offset = typst_source.line_to_byte(line_index)?;
                let utf16_line_offset = typst_source.byte_to_utf16(byte_line_offset)?;
                let utf16_offset = utf16_line_offset + utf16_offset_in_line;

                typst_source.utf16_to_byte(utf16_offset)
            }
        }
    }

    pub fn range(
        lsp_range: LspRange,
        lsp_position_encoding: LspPositionEncoding,
        source: &Source,
    ) -> Option<TypstRange> {
        let lsp_start = lsp_range.start;
        let typst_start = position(lsp_start, lsp_position_encoding, source)?;

        let lsp_end = lsp_range.end;
        let typst_end = position(lsp_end, lsp_position_encoding, source)?;

        Some(TypstRange {
            start: typst_start,
            end: typst_end,
        })
    }
}

pub mod typst_to_lsp {

    use itertools::Itertools;
    use lazy_static::lazy_static;
    use lsp_types::{
        CompletionTextEdit, Documentation, InsertTextFormat, LanguageString, MarkedString,
        MarkupContent, MarkupKind, TextEdit,
    };
    use regex::{Captures, Regex};
    use typst::diag::EcoString;
    use typst::foundations::{CastInfo, Repr};
    use typst::syntax::Source;

    use super::*;

    pub fn offset_to_position(
        typst_offset: TypstOffset,
        lsp_position_encoding: LspPositionEncoding,
        typst_source: &Source,
    ) -> LspPosition {
        let line_index = typst_source.byte_to_line(typst_offset).unwrap();
        let column_index = typst_source.byte_to_column(typst_offset).unwrap();

        let lsp_line = line_index as u32;
        let lsp_column = match lsp_position_encoding {
            LspPositionEncoding::Utf8 => column_index as LspCharacterOffset,
            LspPositionEncoding::Utf16 => {
                // See the implementation of `lsp_to_typst::position_to_offset` for discussion
                // relevant to this function.

                // TODO: Typst's `Source` could easily provide an implementation of the method
                // we   need here. Submit a PR to `typst` to add it, then update
                // this if/when merged.

                let utf16_offset = typst_source.byte_to_utf16(typst_offset).unwrap();

                let byte_line_offset = typst_source.line_to_byte(line_index).unwrap();
                let utf16_line_offset = typst_source.byte_to_utf16(byte_line_offset).unwrap();

                let utf16_column_offset = utf16_offset - utf16_line_offset;
                utf16_column_offset as LspCharacterOffset
            }
        };

        LspPosition::new(lsp_line, lsp_column)
    }

    pub fn range(
        typst_range: TypstRange,
        typst_source: &Source,
        lsp_position_encoding: LspPositionEncoding,
    ) -> LspRange {
        let typst_start = typst_range.start;
        let lsp_start = offset_to_position(typst_start, lsp_position_encoding, typst_source);

        let typst_end = typst_range.end;
        let lsp_end = offset_to_position(typst_end, lsp_position_encoding, typst_source);

        LspRange::new(lsp_start, lsp_end)
    }

    fn completion_kind(typst_completion_kind: TypstCompletionKind) -> LspCompletionKind {
        match typst_completion_kind {
            TypstCompletionKind::Syntax => LspCompletionKind::SNIPPET,
            TypstCompletionKind::Func => LspCompletionKind::FUNCTION,
            TypstCompletionKind::Param => LspCompletionKind::VARIABLE,
            TypstCompletionKind::Constant => LspCompletionKind::CONSTANT,
            TypstCompletionKind::Symbol(_) => LspCompletionKind::FIELD,
            TypstCompletionKind::Type => LspCompletionKind::CLASS,
        }
    }

    lazy_static! {
        static ref TYPST_SNIPPET_PLACEHOLDER_RE: Regex = Regex::new(r"\$\{(.*?)\}").unwrap();
    }

    /// Adds numbering to placeholders in snippets
    fn snippet(typst_snippet: &EcoString) -> String {
        let mut counter = 1;
        let result =
            TYPST_SNIPPET_PLACEHOLDER_RE.replace_all(typst_snippet.as_str(), |cap: &Captures| {
                let substitution = format!("${{{}:{}}}", counter, &cap[1]);
                counter += 1;
                substitution
            });

        result.to_string()
    }

    pub fn completion(typst_completion: &TypstCompletion, lsp_replace: LspRange) -> LspCompletion {
        let typst_snippet = typst_completion
            .apply
            .as_ref()
            .unwrap_or(&typst_completion.label);
        let lsp_snippet = snippet(typst_snippet);
        let text_edit = CompletionTextEdit::Edit(TextEdit::new(lsp_replace, lsp_snippet));

        LspCompletion {
            label: typst_completion.label.to_string(),
            kind: Some(completion_kind(typst_completion.kind.clone())),
            detail: typst_completion.detail.as_ref().map(String::from),
            text_edit: Some(text_edit),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        }
    }

    pub fn completions(
        typst_completions: &[TypstCompletion],
        lsp_replace: LspRange,
    ) -> Vec<LspCompletion> {
        typst_completions
            .iter()
            .map(|typst_completion| completion(typst_completion, lsp_replace))
            .collect_vec()
    }

    pub fn tooltip(typst_tooltip: &TypstTooltip) -> LspHoverContents {
        let lsp_marked_string = match typst_tooltip {
            TypstTooltip::Text(text) => MarkedString::String(text.to_string()),
            TypstTooltip::Code(code) => MarkedString::LanguageString(LanguageString {
                language: "typst".to_owned(),
                value: code.to_string(),
            }),
        };
        LspHoverContents::Scalar(lsp_marked_string)
    }

    pub fn param_info(typst_param_info: &TypstParamInfo) -> LspParamInfo {
        LspParamInfo {
            label: lsp_types::ParameterLabel::Simple(typst_param_info.name.to_owned()),
            documentation: param_info_to_docs(typst_param_info),
        }
    }

    pub fn param_info_to_label(typst_param_info: &TypstParamInfo) -> String {
        format!(
            "{}: {}",
            typst_param_info.name,
            cast_info_to_label(&typst_param_info.input)
        )
    }

    fn param_info_to_docs(typst_param_info: &TypstParamInfo) -> Option<Documentation> {
        if !typst_param_info.docs.is_empty() {
            Some(Documentation::MarkupContent(MarkupContent {
                value: typst_param_info.docs.to_owned(),
                kind: MarkupKind::Markdown,
            }))
        } else {
            None
        }
    }

    pub fn cast_info_to_label(cast_info: &CastInfo) -> String {
        match cast_info {
            CastInfo::Any => "any".to_owned(),
            CastInfo::Value(value, _) => value.repr().to_string(),
            CastInfo::Type(ty) => ty.to_string(),
            CastInfo::Union(options) => options.iter().map(cast_info_to_label).join(" "),
        }
    }
}

#[cfg(test)]
mod test {
    use lsp_types::Position;
    use typst::syntax::Source;

    use crate::{lsp_to_typst, PositionEncoding};

    use super::*;

    #[test]
    fn test_untitled() {
        let path = Path::new("/untitled/test");
        let uri = path_to_url(path).unwrap();
        assert_eq!(uri.scheme(), "untitled");
        assert_eq!(uri.path(), "test");

        let path = url_to_path(uri);
        assert_eq!(path, Path::new("/untitled/test").clean());
    }

    #[test]
    fn unnamed_buffer() {
        // https://github.com/neovim/nvim-lspconfig/pull/2226
        let uri = EMPTY_URL.clone();
        let path = url_to_path(uri);
        assert_eq!(path, Path::new("/untitled/neovim-bug"));

        let uri2 = path_to_url(&path).unwrap();
        assert_eq!(EMPTY_URL.clone(), uri2);
    }

    const ENCODING_TEST_STRING: &str = "test 🥺 test";

    #[test]
    fn issue_14_invalid_range() {
        let source = Source::detached("#set page(height: 2cm)");
        let rng = LspRange {
            start: LspPosition {
                line: 0,
                character: 22,
            },
            // EOF
            end: LspPosition {
                line: 1,
                character: 0,
            },
        };
        let res = lsp_to_typst::range(rng, PositionEncoding::Utf16, &source).unwrap();
        assert_eq!(res, 22..22);
    }

    #[test]
    fn issue_14_invalid_range_2() {
        let source = Source::detached(
            r"#let f(a) = {
  a
}
",
        );
        let rng = LspRange {
            start: LspPosition {
                line: 2,
                character: 1,
            },
            // EOF
            end: LspPosition {
                line: 3,
                character: 0,
            },
        };
        let res = lsp_to_typst::range(rng, PositionEncoding::Utf16, &source).unwrap();
        assert_eq!(res, 19..source.len_bytes());
        // EOF
        let rng = LspRange {
            start: LspPosition {
                line: 3,
                character: 1,
            },
            end: LspPosition {
                line: 4,
                character: 0,
            },
        };
        let res = lsp_to_typst::range(rng, PositionEncoding::Utf16, &source).unwrap();
        assert_eq!(res, source.len_bytes()..source.len_bytes());

        for line in 0..=5 {
            for character in 0..2 {
                let off = lsp_to_typst::position(
                    Position { line, character },
                    PositionEncoding::Utf16,
                    &source,
                );
                assert!(off.is_some(), "line: {line}, character: {character}");
            }
        }
    }

    #[test]
    fn utf16_position_to_utf8_offset() {
        let source = Source::detached(ENCODING_TEST_STRING);

        let start = LspPosition {
            line: 0,
            character: 0,
        };
        let emoji = LspPosition {
            line: 0,
            character: 5,
        };
        let post_emoji = LspPosition {
            line: 0,
            character: 7,
        };
        let end = LspPosition {
            line: 0,
            character: 12,
        };

        let start_offset = lsp_to_typst::position(start, PositionEncoding::Utf16, &source).unwrap();
        let start_actual = 0;

        let emoji_offset = lsp_to_typst::position(emoji, PositionEncoding::Utf16, &source).unwrap();
        let emoji_actual = 5;

        let post_emoji_offset =
            lsp_to_typst::position(post_emoji, PositionEncoding::Utf16, &source).unwrap();
        let post_emoji_actual = 9;

        let end_offset = lsp_to_typst::position(end, PositionEncoding::Utf16, &source).unwrap();
        let end_actual = 14;

        assert_eq!(start_offset, start_actual);
        assert_eq!(emoji_offset, emoji_actual);
        assert_eq!(post_emoji_offset, post_emoji_actual);
        assert_eq!(end_offset, end_actual);
    }

    #[test]
    fn utf8_offset_to_utf16_position() {
        let source = Source::detached(ENCODING_TEST_STRING);

        let start = 0;
        let emoji = 5;
        let post_emoji = 9;
        let end = 14;

        let start_position = LspPosition {
            line: 0,
            character: 0,
        };
        let start_actual =
            typst_to_lsp::offset_to_position(start, PositionEncoding::Utf16, &source);

        let emoji_position = LspPosition {
            line: 0,
            character: 5,
        };
        let emoji_actual =
            typst_to_lsp::offset_to_position(emoji, PositionEncoding::Utf16, &source);

        let post_emoji_position = LspPosition {
            line: 0,
            character: 7,
        };
        let post_emoji_actual =
            typst_to_lsp::offset_to_position(post_emoji, PositionEncoding::Utf16, &source);

        let end_position = LspPosition {
            line: 0,
            character: 12,
        };
        let end_actual = typst_to_lsp::offset_to_position(end, PositionEncoding::Utf16, &source);

        assert_eq!(start_position, start_actual);
        assert_eq!(emoji_position, emoji_actual);
        assert_eq!(post_emoji_position, post_emoji_actual);
        assert_eq!(end_position, end_actual);
    }
}
