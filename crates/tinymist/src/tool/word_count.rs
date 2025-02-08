//! Word count tool for documents.

use std::ops::Range;
use std::sync::Arc;

use reflexo_typst::debug_loc::SourceSpanOffset;
use serde::{Deserialize, Serialize};
use tinymist_std::typst::TypstDocument;
use tinymist_task::TextExport;
use typst::{syntax::Span, text::TextItem};
use unicode_script::{Script, UnicodeScript};

/// Words count for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordsCount {
    /// Number of words.
    pub words: usize,
    /// Number of characters.
    pub chars: usize,
    /// Number of spaces.
    /// Multiple consecutive spaces are counted as one.
    pub spaces: usize,
    /// Number of CJK characters.
    pub cjk_chars: usize,
}

/// Count words in a document.
pub fn word_count(doc: &TypstDocument) -> WordsCount {
    // the mapping is still not use, so we prevent the warning here
    let _ = TextContent::map_back_spans;

    let mut words = 0;
    let mut chars = 0;
    let mut cjk_chars = 0;
    let mut spaces = 0;

    // First, get text representation of the document.
    let content = TextExport::run_on_doc(doc).unwrap_or_default();

    /// A automaton to count words.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CountState {
        /// Waiting for a word. (Default state)
        InSpace,
        /// At a word.
        InNonCJK,
        /// At a CJK character.
        InCJK,
    }

    fn is_cjk(c: char) -> bool {
        matches!(
            c.script(),
            Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul
        )
    }

    let mut state = CountState::InSpace;
    for c in content.chars() {
        chars += 1;

        if c.is_whitespace() {
            if state != CountState::InSpace {
                spaces += 1;
            }
            state = CountState::InSpace;
            continue;
        }

        // Check unicode script to see if it's a CJK character.
        if is_cjk(c) {
            words += 1;
            cjk_chars += 1;

            state = CountState::InCJK;
        } else {
            if state != CountState::InNonCJK {
                words += 1;
            }

            state = CountState::InNonCJK;
        }
    }

    WordsCount {
        words,
        chars,
        spaces,
        cjk_chars,
    }
}

/// Given a text range, map it back to the original document.
#[derive(Debug, Clone)]
pub struct MappedSpan {
    /// The start span.
    pub span: SourceSpanOffset,
    /// The end span.
    pub span_end: Option<SourceSpanOffset>,
    /// Whether a text range is completely covered by [`Self::span`] and
    /// [`Self::span_end`].
    pub completed: bool,
}

/// Annotated content for a font.
#[derive(Debug, Clone)]
pub struct TextContent {
    /// A string of the content for slicing.
    pub content: String,
    /// annotating document.
    pub doc: Arc<TypstDocument>,
}

impl TextContent {
    /// Map text ranges (with byte offsets) back to the original document in
    /// batch.
    pub fn map_back_spans(
        &self,
        mut spans: Vec<std::ops::Range<usize>>,
    ) -> Vec<Option<MappedSpan>> {
        // Sort for scanning
        spans.sort_by_key(|r| r.start);

        // Scan the document recursively to map back the spans.
        let mut mapper = SpanMapper::default();
        mapper.doc(&self.doc);

        // Align result with the input to prevent bad scanning.
        let mut offsets = mapper.span_offset;
        while spans.len() < offsets.len() {
            offsets.pop();
        }
        while spans.len() > offsets.len() {
            offsets.push(None);
        }

        offsets
    }
}

#[derive(Debug, Clone, Default)]
struct SpanMapper {
    offset: usize,
    spans_to_map: Vec<std::ops::Range<usize>>,
    span_offset: Vec<Option<MappedSpan>>,
}

impl SpanMapper {
    fn doc(&mut self, doc: &TypstDocument) {
        match doc {
            TypstDocument::Paged(paged_doc) => {
                for page in paged_doc.pages.iter() {
                    self.frame(&page.frame);
                }
            }
        }
    }

    fn frame(&mut self, doc: &typst::layout::Frame) {
        for (_, item) in doc.items() {
            self.item(item);
        }
    }

    fn item(&mut self, item: &typst::layout::FrameItem) {
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => self.frame(&g.frame),
            Text(t) => {
                self.check(t.text.as_str(), Some(t));
            }
            Link(..) | Shape(..) | Image(..) => {
                self.check("object", None);
            }
            Tag(..) => {}
        }
    }

    fn check(&mut self, text: &str, src: Option<&TextItem>) {
        if let Some(src) = src {
            self.do_check(src);
        }
        self.offset += text.len();
    }

    fn do_check(&mut self, text: &TextItem) -> Option<()> {
        let beg = self.offset;
        let end = beg + text.text.len();
        loop {
            let so = self.span_offset.len();
            let to_check = self.spans_to_map.get(so)?;
            if to_check.start >= end {
                return Some(());
            }
            if to_check.end <= beg {
                self.span_offset.push(None);
                log::info!("span out of range {to_check:?}");
                continue;
            }
            // todo: don't swallow the span
            if to_check.start < beg {
                self.span_offset.push(None);
                log::info!("span skipped {to_check:?}");
                continue;
            }

            log::info!("span checking {to_check:?} in {text:?}");
            let inner = to_check.start - beg;
            self.span_offset
                .push(self.check_text_inner(text, inner..inner + (to_check.len())));
        }
    }

    fn check_text_inner(&self, text: &TextItem, rng: std::ops::Range<usize>) -> Option<MappedSpan> {
        let glyphs = text
            .glyphs
            .iter()
            .filter(|g| rng.contains(&g.range().start));
        let mut min_span: Option<(Range<usize>, (Span, u16))> = None;
        let mut max_span: Option<(Range<usize>, (Span, u16))> = None;
        let mut found = vec![];
        for glyph in glyphs {
            found.push(glyph.range());
            if let Some((mii, s)) = min_span.as_ref() {
                if glyph.range().start < mii.start && !s.0.is_detached() {
                    // min_span = Some(glyph.range());
                    min_span = Some((glyph.range(), glyph.span));
                }
            } else {
                // min_span = Some(glyph.range());
                min_span = Some((glyph.range(), glyph.span));
            }
            if let Some((mai, s)) = max_span.as_ref() {
                if glyph.range().end > mai.end && !s.0.is_detached() {
                    // max_span = Some(glyph.range());
                    max_span = Some((glyph.range(), glyph.span));
                }
            } else {
                // max_span = Some(glyph.range());
                max_span = Some((glyph.range(), glyph.span));
            }
        }
        found.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.end.cmp(&b.end)));
        let completed = !found.is_empty()
            && found[0].start <= rng.start
            && found[found.len() - 1].end >= rng.end;
        let span = min_span?.1 .0;
        let span_end = max_span.map(|m| m.1 .0);
        Some(MappedSpan {
            span: SourceSpanOffset::from(span),
            span_end: span_end.map(SourceSpanOffset::from),
            completed,
        })
    }
}
