//! SyncTeX output for paged documents.
//!
//! SyncTeX is how LaTeX editors and PDF viewers link a PDF to its sources: a
//! file next to the PDF lists boxes on each page with the source file and line
//! they came from. Viewers use it to show the PDF position of an editor line
//! (forward search) and to open the source line of a clicked point (backward
//! search). Nothing in these files is specific to LaTeX, so by writing one we
//! get both features for Typst sources, without any change to the viewers.
//!
//! The format is documented in `synctex(5)`, see
//! <https://github.com/jlaurens/synctex/blob/main/man5/synctex.5>, the design
//! in J. Laurens, "Direct and reverse synchronization with SyncTeX", TUGboat
//! 29:3 (2008), <https://tug.org/TUGboat/tb29-3/tb93laurens.pdf>. The reference
//! reader is `synctex_parser`, which the `synctex` command-line tool of TeX
//! Live wraps. Lengths are in scaled points (sp), with y growing downwards; for
//! an A4 page with one text run, this generator writes:
//!
//! ```text
//! SyncTeX Version:1
//! Input:1:/path/to/main.typ        one per source file, numbered by tag
//! Output:pdf
//! Magnification:1000
//! Unit:1
//! X Offset:0
//! Y Offset:0
//! Content:
//! {1                               page 1
//! [1,2:0,55380990:39158276,55380990,0
//!                                  page box:  tag,line:x,y:width,height,depth
//! (1,2:4736287,6578176:19734528,592036,197345
//!                                  text run:  same fields, x,y at the baseline
//! )
//! ]
//! }1
//! Postamble:
//! Count:2                          number of box records
//! Post scriptum:
//! ```
//!
//! This is a subset of what pdfTeX writes: there are no `!` byte-offset
//! anchors, which only speed up reading, and no records for glue, kerns or
//! math. pdfTeX counts all records in `Count`; the reader does not check it.
//!
//! Every text run becomes one hbox record carrying the file tag and line of its
//! glyphs; runs are split where the source line changes. Glyphs with a detached
//! span, such as the text of a reference, have no source and get no record. A
//! run's box is cut to the bounding box of its clip, and a run clipped away
//! entirely gets no record. A page without records has no page box either, so
//! a click there finds nothing. Outline entries are skipped: their text carries
//! the heading's span, and a viewer would otherwise jump to the table of
//! contents instead of the heading.
//!
//! The file is gzip-compressed like the `.synctex.gz` files of `pdflatex
//! -synctex=1`. The text compresses by a factor of about 5, and viewers read
//! both forms.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::ops::Range;

use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstPagedDocument;
use tinymist_world::{CompilerFeat, CompilerWorld};
use typst::World;
use typst::foundations::{Bytes, NativeElement};
use typst::introspection::{Location, Tag};
use typst::layout::{Abs, Frame, FrameItem, Point, Rect, Transform};
use typst::model::OutlineEntry;
use typst::syntax::{FileId, LinkedNode, Source, Span, SyntaxKind};

/// Scaled points per PostScript point: 65536 sp per TeX point, 72.27 TeX
/// points per 72 PostScript points.
const SP_PER_PT: f64 = 65536.0 * 72.27 / 72.0;

/// Generates the gzip-compressed SyncTeX data for a document.
pub fn synctex_gz<F: CompilerFeat>(
    world: &CompilerWorld<F>,
    doc: &TypstPagedDocument,
) -> Result<Bytes> {
    let mut writer = SyncTexGen {
        world,
        files: Vec::new(),
        sources: HashMap::new(),
        nodes: HashMap::new(),
        content: String::new(),
        records: 0,
        open_entries: Vec::new(),
    };

    for (idx, page) in doc.pages().iter().enumerate() {
        writer.page(idx + 1, &page.frame);
    }

    let mut out = String::new();
    out.push_str("SyncTeX Version:1\n");
    for (tag, path) in writer.files.iter().enumerate() {
        let _ = writeln!(out, "Input:{}:{path}", tag + 1);
    }
    out.push_str("Output:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n");
    out.push_str(&writer.content);
    let _ = write!(
        out,
        "Postamble:\nCount:{}\nPost scriptum:\n",
        writer.records
    );

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(out.as_bytes())
        .and_then(|_| encoder.finish())
        .map(Bytes::new)
        .context("compress synctex data")
}

struct SyncTexGen<'a, F: CompilerFeat> {
    world: &'a CompilerWorld<F>,
    /// Absolute paths, indexed by tag - 1.
    files: Vec<String>,
    /// Each file seen so far, with its tag; `None` if it has no source or path.
    sources: HashMap<FileId, Option<(usize, Source)>>,
    /// The syntax node of every span in those files. Indexing a file once is
    /// linear in its size, while `Source::find` searches from the root for each
    /// span, which is quadratic for text with many spans, such as inline math.
    nodes: HashMap<Span, SpanNode>,
    content: String,
    records: usize,
    /// Outline entries whose end tag has not been seen yet.
    open_entries: Vec<Location>,
}

/// The byte range of a span's node, and whether it is text whose glyph offsets
/// index into that range.
struct SpanNode {
    range: Range<usize>,
    is_text: bool,
}

/// A text run's source location and its box in page coordinates.
struct Run {
    tag: usize,
    line: usize,
    x: Abs,
    baseline: Abs,
    width: Abs,
    height: Abs,
    depth: Abs,
}

impl Run {
    /// Maps the local box `local`, whose baseline is at `baseline`, to page
    /// coordinates, and cuts it to `clip`. SyncTeX boxes are upright, so a
    /// rotated box becomes its bounding box, with the baseline kept inside. A
    /// box clipped away entirely gives `None`.
    fn new(
        (tag, line): (usize, usize),
        ts: Transform,
        clip: Option<Rect>,
        local: Rect,
        baseline: Abs,
    ) -> Option<Self> {
        let Rect { min, max } = intersect(bounds(local, ts), clip)?;
        let baseline = Point::new(local.min.x, baseline)
            .transform(ts)
            .y
            .clamp(min.y, max.y);

        Some(Run {
            tag,
            line,
            x: min.x,
            baseline,
            width: max.x - min.x,
            height: baseline - min.y,
            depth: max.y - baseline,
        })
    }
}

/// The upright bounding box of `rect` after transforming it by `ts`.
fn bounds(rect: Rect, ts: Transform) -> Rect {
    let corners = [
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        Point::new(rect.min.x, rect.max.y),
        rect.max,
    ]
    .map(|corner| corner.transform(ts));
    Rect::new(
        corners.into_iter().reduce(Point::min).unwrap_or_default(),
        corners.into_iter().reduce(Point::max).unwrap_or_default(),
    )
}

/// The part of `rect` inside `clip`, or `None` if nothing of it is visible.
fn intersect(rect: Rect, clip: Option<Rect>) -> Option<Rect> {
    let Some(clip) = clip else {
        return Some(rect);
    };
    let min = rect.min.max(clip.min);
    let max = rect.max.min(clip.max);
    (min.x < max.x && min.y < max.y).then(|| Rect::new(min, max))
}

impl<F: CompilerFeat> SyncTexGen<'_, F> {
    fn page(&mut self, number: usize, frame: &Frame) {
        let mut runs = Vec::new();
        self.collect(frame, Transform::identity(), None, &mut runs);

        let _ = writeln!(self.content, "{{{number}");

        // The page vbox takes the location of its first run so that it maps to
        // a meaningful line. A page without runs gets no box, since any location
        // would be invented; the reader then finds nothing there.
        if let Some(first) = runs.first() {
            let (tag, line) = (first.tag, first.line);
            let w = sp(frame.width());
            let h = sp(frame.height());
            let _ = writeln!(self.content, "[{tag},{line}:0,{h}:{w},{h},0");
            self.records += 1;

            for run in runs {
                let _ = writeln!(
                    self.content,
                    "({},{}:{},{}:{},{},{}\n)",
                    run.tag,
                    run.line,
                    sp(run.x),
                    sp(run.baseline),
                    sp(run.width),
                    sp(run.height),
                    sp(run.depth)
                );
                self.records += 1;
            }

            let _ = writeln!(self.content, "]");
        }

        let _ = writeln!(self.content, "}}{number}");
    }

    /// Collects the text runs of `frame`, placed on the page by `ts` and visible
    /// only within `clip`, in page coordinates.
    fn collect(&mut self, frame: &Frame, ts: Transform, clip: Option<Rect>, runs: &mut Vec<Run>) {
        for &(pos, ref item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    let ts = ts
                        .pre_concat(Transform::translate(pos.x, pos.y))
                        .pre_concat(group.transform);

                    // The clip curve is in the group's coordinates. A group
                    // clipped away entirely has no visible text.
                    let clip = match &group.clip {
                        Some(curve) => match intersect(bounds(curve.bbox(None), ts), clip) {
                            Some(clip) => Some(clip),
                            None => continue,
                        },
                        None => clip,
                    };
                    self.collect(&group.frame, ts, clip, runs);
                }
                FrameItem::Tag(Tag::Start(content, _)) => {
                    if content.elem() == OutlineEntry::ELEM
                        && let Some(loc) = content.location()
                    {
                        self.open_entries.push(loc);
                    }
                }
                FrameItem::Tag(Tag::End(loc, _, _)) => {
                    if self.open_entries.last() == Some(loc) {
                        self.open_entries.pop();
                    }
                }
                FrameItem::Text(_) if !self.open_entries.is_empty() => {}
                FrameItem::Text(text) => {
                    let metrics = text.font.metrics();
                    let ascent = metrics.ascender.at(text.size);
                    let descent = -metrics.descender.at(text.size);

                    // The current run: its location and where it starts.
                    let mut current: Option<((usize, usize), Abs)> = None;
                    let mut x = pos.x;
                    let finish = |current: Option<_>, end, runs: &mut Vec<Run>| {
                        let Some((loc, start)) = current else {
                            return;
                        };
                        let local = Rect::new(
                            Point::new(start, pos.y - ascent),
                            Point::new(end, pos.y + descent),
                        );
                        runs.extend(Run::new(loc, ts, clip, local, pos.y));
                    };
                    for glyph in &text.glyphs {
                        let loc = self.locate(glyph.span.0, glyph.span.1 as usize);
                        if current.map(|(loc, _)| loc) != loc {
                            finish(current, x, runs);
                            current = loc.map(|loc| (loc, x));
                        }
                        x += glyph.x_advance.at(text.size);
                    }
                    finish(current, x, runs);
                }
                _ => {}
            }
        }
    }

    /// Resolves a glyph's span and offset to its file tag and 1-based line.
    fn locate(&mut self, span: Span, offset: usize) -> Option<(usize, usize)> {
        let id = span.id()?;
        if !self.sources.contains_key(&id) {
            let entry = self.register(id);
            self.sources.insert(id, entry);
        }
        let (tag, source) = self.sources.get(&id)?.as_ref()?;
        let node = self.nodes.get(&span)?;

        // Only text renders its own source, so only there does the offset point
        // into the node. Other glyphs, such as those of `#lorem(50)` or of a
        // string expression, map to the expression that generated them. This follows
        // `jump_from_click` in `typst-ide`.
        let byte = if node.is_text {
            (node.range.start + offset).min(node.range.end)
        } else {
            node.range.start
        };
        let line = source.lines().byte_to_line(byte)?;
        Some((*tag, line + 1))
    }

    /// Assigns the next tag to a file and indexes the spans of its syntax tree.
    fn register(&mut self, id: FileId) -> Option<(usize, Source)> {
        let source = self.world.source(id).ok()?;
        let path = self.world.path_for_id(id).ok()?;
        self.files
            .push(path.as_path().to_string_lossy().into_owned());

        let mut stack = vec![LinkedNode::new(source.root())];
        while let Some(node) = stack.pop() {
            self.nodes.insert(
                node.span(),
                SpanNode {
                    range: node.range(),
                    is_text: matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText),
                },
            );
            stack.extend(node.children());
        }

        Some((self.files.len(), source))
    }
}

fn sp(len: Abs) -> i64 {
    (len.to_pt() * SP_PER_PT).round() as i64
}
