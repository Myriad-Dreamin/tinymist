//! Conversions between Typst and LSP locations

use std::cmp::Ordering;
use std::ops::Range;

use typst::syntax::Source;

/// An LSP Position encoded by [`PositionEncoding`].
type LspPosition = tinymist_world::debug_loc::LspPosition;
/// An LSP range encoded by [`PositionEncoding`].
type LspRange = tinymist_world::debug_loc::LspRange;

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

impl From<PositionEncoding> for tinymist_world::debug_loc::PositionEncodingKind {
    fn from(position_encoding: PositionEncoding) -> Self {
        match position_encoding {
            PositionEncoding::Utf16 => Self::UTF16,
            PositionEncoding::Utf8 => Self::UTF8,
        }
    }
}

/// Convert an LSP position to a Typst position.
pub fn to_typst_position(
    lsp_position: LspPosition,
    lsp_position_encoding: PositionEncoding,
    typst_source: &Source,
) -> Option<usize> {
    let lines = typst_source.len_lines() as u32;

    'bound_checking: {
        let should_warning = match lsp_position.line.cmp(&lines) {
            Ordering::Greater => true,
            Ordering::Equal => lsp_position.character > 0,
            Ordering::Less if lsp_position.line + 1 == lines => {
                let last_line_offset = typst_source.line_to_byte(lines as usize - 1)?;
                let last_line_chars = &typst_source.text()[last_line_offset..];
                let len = match lsp_position_encoding {
                    PositionEncoding::Utf8 => last_line_chars.len(),
                    PositionEncoding::Utf16 => {
                        last_line_chars.chars().map(char::len_utf16).sum::<usize>()
                    }
                };

                match lsp_position.character.cmp(&(len as u32)) {
                    Ordering::Less => break 'bound_checking,
                    Ordering::Greater => true,
                    Ordering::Equal => false,
                }
            }
            Ordering::Less => break 'bound_checking,
        };

        if should_warning {
            log::warn!(
                    "LSP position is out of bounds: {:?}, while only {:?} lines and {:?} characters at the end.",
                    lsp_position, typst_source.len_lines(), typst_source.line_to_range(typst_source.len_lines() - 1),
                );
        }

        return Some(typst_source.len_bytes());
    }

    match lsp_position_encoding {
        PositionEncoding::Utf8 => {
            let line_index = lsp_position.line as usize;
            let column_index = lsp_position.character as usize;
            typst_source.line_column_to_byte(line_index, column_index)
        }
        PositionEncoding::Utf16 => {
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

/// Convert a Typst position to an LSP position.
pub fn to_lsp_position(
    typst_offset: usize,
    lsp_position_encoding: PositionEncoding,
    typst_source: &Source,
) -> LspPosition {
    if typst_offset > typst_source.len_bytes() {
        return LspPosition::new(typst_source.len_lines() as u32, 0);
    }

    let line_index = typst_source.byte_to_line(typst_offset).unwrap();
    let column_index = typst_source.byte_to_column(typst_offset).unwrap();

    let lsp_line = line_index as u32;
    let lsp_column = match lsp_position_encoding {
        PositionEncoding::Utf8 => column_index as u32,
        PositionEncoding::Utf16 => {
            // See the implementation of `position_to_offset` for discussion
            // relevant to this function.

            // TODO: Typst's `Source` could easily provide an implementation of the method
            // we   need here. Submit a PR to `typst` to add it, then update
            // this if/when merged.

            let utf16_offset = typst_source.byte_to_utf16(typst_offset).unwrap();

            let byte_line_offset = typst_source.line_to_byte(line_index).unwrap();
            let utf16_line_offset = typst_source.byte_to_utf16(byte_line_offset).unwrap();

            let utf16_column_offset = utf16_offset - utf16_line_offset;
            utf16_column_offset as u32
        }
    };

    LspPosition::new(lsp_line, lsp_column)
}

/// Convert an LSP range to a Typst range.
pub fn to_typst_range(
    lsp_range: LspRange,
    lsp_position_encoding: PositionEncoding,
    source: &Source,
) -> Option<Range<usize>> {
    let lsp_start = lsp_range.start;
    let typst_start = to_typst_position(lsp_start, lsp_position_encoding, source)?;

    let lsp_end = lsp_range.end;
    let typst_end = to_typst_position(lsp_end, lsp_position_encoding, source)?;

    Some(Range {
        start: typst_start,
        end: typst_end,
    })
}

/// Convert a Typst range to an LSP range.
pub fn to_lsp_range(
    typst_range: Range<usize>,
    typst_source: &Source,
    lsp_position_encoding: PositionEncoding,
) -> LspRange {
    let typst_start = typst_range.start;
    let lsp_start = to_lsp_position(typst_start, lsp_position_encoding, typst_source);

    let typst_end = typst_range.end;
    let lsp_end = to_lsp_position(typst_end, lsp_position_encoding, typst_source);

    LspRange::new(lsp_start, lsp_end)
}

#[cfg(test)]
mod test {
    use super::LspPosition as Position;

    use super::*;

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
        let res = to_typst_range(rng, PositionEncoding::Utf16, &source).unwrap();
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
        let res = to_typst_range(rng, PositionEncoding::Utf16, &source).unwrap();
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
        let res = to_typst_range(rng, PositionEncoding::Utf16, &source).unwrap();
        assert_eq!(res, source.len_bytes()..source.len_bytes());

        for line in 0..=5 {
            for character in 0..2 {
                let off = to_typst_position(
                    Position { line, character },
                    PositionEncoding::Utf16,
                    &source,
                );
                assert!(off.is_some(), "line: {line}, character: {character}");
            }
        }
    }

    #[test]
    fn overflow_offset_to_position() {
        let source = Source::detached("test");

        let offset = source.len_bytes();
        let position = to_lsp_position(offset, PositionEncoding::Utf16, &source);
        assert_eq!(
            position,
            LspPosition {
                line: 0,
                character: 4
            }
        );

        let offset = source.len_bytes() + 1;
        let position = to_lsp_position(offset, PositionEncoding::Utf16, &source);
        assert_eq!(
            position,
            LspPosition {
                line: 1,
                character: 0
            }
        );
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

        let start_offset = to_typst_position(start, PositionEncoding::Utf16, &source).unwrap();
        let start_actual = 0;

        let emoji_offset = to_typst_position(emoji, PositionEncoding::Utf16, &source).unwrap();
        let emoji_actual = 5;

        let post_emoji_offset =
            to_typst_position(post_emoji, PositionEncoding::Utf16, &source).unwrap();
        let post_emoji_actual = 9;

        let end_offset = to_typst_position(end, PositionEncoding::Utf16, &source).unwrap();
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
        let start_actual = to_lsp_position(start, PositionEncoding::Utf16, &source);

        let emoji_position = LspPosition {
            line: 0,
            character: 5,
        };
        let emoji_actual = to_lsp_position(emoji, PositionEncoding::Utf16, &source);

        let post_emoji_position = LspPosition {
            line: 0,
            character: 7,
        };
        let post_emoji_actual = to_lsp_position(post_emoji, PositionEncoding::Utf16, &source);

        let end_position = LspPosition {
            line: 0,
            character: 12,
        };
        let end_actual = to_lsp_position(end, PositionEncoding::Utf16, &source);

        assert_eq!(start_position, start_actual);
        assert_eq!(emoji_position, emoji_actual);
        assert_eq!(post_emoji_position, post_emoji_actual);
        assert_eq!(end_position, end_actual);
    }
}
