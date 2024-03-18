//! Conversions between Typst and LSP types and representations

use lsp_types;

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
pub type TypstCompletion = typst_ide::Completion;
pub type TypstCompletionKind = typst_ide::CompletionKind;

pub mod lsp_to_typst {
    use typst::syntax::Source;

    use super::*;

    pub fn position(
        lsp_position: LspPosition,
        lsp_position_encoding: LspPositionEncoding,
        typst_source: &Source,
    ) -> Option<TypstOffset> {
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
            TypstCompletionKind::Symbol(_) => LspCompletionKind::TEXT,
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
    use typst::syntax::Source;

    use crate::{lsp_to_typst, PositionEncoding};

    use super::*;

    const ENCODING_TEST_STRING: &str = "test 🥺 test";

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
