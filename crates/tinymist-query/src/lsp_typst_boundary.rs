//! Conversions between Typst and LSP types and representations

use std::cmp::Ordering;

use tinymist_std::path::PathClean;
use typst::syntax::Source;

use crate::prelude::*;

/// An LSP Position encoded by [`PositionEncoding`].
pub type LspPosition = lsp_types::Position;
/// An LSP range encoded by [`PositionEncoding`].
pub type LspRange = lsp_types::Range;

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

const UNTITLED_ROOT: &str = "/untitled";
static EMPTY_URL: LazyLock<Url> = LazyLock::new(|| Url::parse("file://").unwrap());

/// Convert a path to a URL.
pub fn path_to_url(path: &Path) -> anyhow::Result<Url> {
    if let Ok(untitled) = path.strip_prefix(UNTITLED_ROOT) {
        // rust-url will panic on converting an empty path.
        if untitled == Path::new("nEoViM-BuG") {
            return Ok(EMPTY_URL.clone());
        }

        return Ok(Url::parse(&format!("untitled:{}", untitled.display()))?);
    }

    Url::from_file_path(path).or_else(|never| {
        let _: () = never;

        anyhow::bail!("could not convert path to URI: path: {path:?}",)
    })
}

/// Convert a URL to a path.
pub fn url_to_path(uri: Url) -> PathBuf {
    if uri.scheme() == "file" {
        // typst converts an empty path to `Path::new("/")`, which is undesirable.
        if !uri.has_host() && uri.path() == "/" {
            return PathBuf::from("/untitled/nEoViM-BuG");
        }

        return uri
            .to_file_path()
            .unwrap_or_else(|_| panic!("could not convert URI to path: URI: {uri:?}",));
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
    use lsp_types::Position;

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
        assert_eq!(path, Path::new("/untitled/nEoViM-BuG"));

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
