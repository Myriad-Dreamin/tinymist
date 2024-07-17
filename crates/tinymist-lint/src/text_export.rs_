use std::io::{self, Write};
use std::ops::Range;
use std::sync::Arc;

use typst::syntax::Span;
use typst::text::TextItem;
use typst_ts_core::debug_loc::SourceSpanOffset;
use typst_ts_core::exporter_utils::map_err;
use typst_ts_core::TypstDocument;

#[derive(Debug, Clone, Default)]
pub struct TextExporter {}

#[derive(Debug, Clone)]
pub struct MappedSpan {
    pub span: SourceSpanOffset,
    pub span_end: Option<SourceSpanOffset>,
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
    pub fn map_back_spans(
        &self,
        mut spans: Vec<std::ops::Range<usize>>,
    ) -> Vec<Option<MappedSpan>> {
        // sort
        spans.sort_by_key(|r| r.start);

        let mut mapper = SpanMapper::default();
        mapper.doc(&self.doc);

        // mapper.span_offset
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

impl TextExporter {
    pub fn annotate(&self, output: Arc<TypstDocument>) -> typst::diag::SourceResult<TextContent> {
        let w = std::io::BufWriter::new(Vec::new());

        let mut d = FullTextDigest { w };
        d.doc(&output).map_err(map_err)?;

        d.w.flush().unwrap();
        Ok(TextContent {
            content: String::from_utf8(d.w.into_inner().unwrap()).unwrap(),
            doc: output,
        })
    }
}

struct FullTextDigest {
    w: std::io::BufWriter<Vec<u8>>,
}

impl FullTextDigest {
    fn doc(&mut self, doc: &TypstDocument) -> io::Result<()> {
        for page in doc.pages.iter() {
            self.frame(&page.frame)?;
        }
        Ok(())
    }

    fn frame(&mut self, doc: &typst::layout::Frame) -> io::Result<()> {
        for (_, item) in doc.items() {
            self.item(item)?;
        }

        Ok(())
    }

    fn item(&mut self, item: &typst::layout::FrameItem) -> io::Result<()> {
        use typst::introspection::Meta::*;
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => self.frame(&g.frame),
            Text(t) => {
                write!(self.w, "{}", t.text.as_str())
            }
            // Meta(ContentHint(c), _) => f.write_char(*c),
            Meta(Link(..), _) | Shape(..) | Image(..) => self.w.write_all(b"object"),
            Meta(Elem(..) | Hide, _) => Ok(()),
        }
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
        for page in doc.pages.iter() {
            self.frame(&page.frame);
        }
    }

    fn frame(&mut self, doc: &typst::layout::Frame) {
        for (_, item) in doc.items() {
            self.item(item);
        }
    }

    fn item(&mut self, item: &typst::layout::FrameItem) {
        use typst::introspection::Meta::*;
        use typst::layout::FrameItem::*;
        match item {
            Group(g) => self.frame(&g.frame),
            Text(t) => {
                self.check(t.text.as_str(), Some(t));
            }
            Meta(Link(..), _) | Shape(..) | Image(..) => {
                self.check("object", None);
            }
            Meta(Elem(..) | Hide, _) => {}
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
