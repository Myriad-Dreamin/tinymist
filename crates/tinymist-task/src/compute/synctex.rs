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
//! Live wraps. Lengths are in scaled points; this generator writes:
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
//! [1,2:0,842:595,842,0             page box:  tag,line:x,y:width,height,depth
//! (1,2:72,100:300,9,3              text run:  same fields, x,y at the baseline
//! )
//! ]
//! }1
//! Postamble:
//! Count:2                          number of boxes
//! Post scriptum:
//! ```
//!
//! Every text run becomes one hbox record carrying the file tag and line of its
//! glyphs; runs are split where the source line changes. Outline entries are
//! skipped: their text carries the heading's span, and a viewer would otherwise
//! jump to the table of contents instead of the heading.
//!
//! The file is gzip-compressed like the `.synctex.gz` files of `pdflatex
//! -synctex=1`. The text compresses by a factor of about 5, and viewers read
//! both forms.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write as _;

use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstPagedDocument;
use tinymist_world::{CompilerFeat, CompilerWorld};
use typst::World;
use typst::foundations::{Bytes, NativeElement};
use typst::introspection::{Location, Tag};
use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst::model::OutlineEntry;
use typst::syntax::{FileId, Source};

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
        tags: HashMap::new(),
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
    tags: HashMap<FileId, Option<(usize, Source)>>,
    content: String,
    records: usize,
    /// Outline entries whose end tag has not been seen yet.
    open_entries: Vec<Location>,
}

/// A text run's source location and extent.
struct Run {
    tag: usize,
    line: usize,
    origin: Point,
    width: Abs,
    ascent: Abs,
    descent: Abs,
}

impl<F: CompilerFeat> SyncTexGen<'_, F> {
    fn page(&mut self, number: usize, frame: &Frame) {
        let mut runs = Vec::new();
        self.collect(frame, Transform::identity(), &mut runs);

        // The page vbox takes the location of its first run so that it maps to
        // a meaningful line.
        let (tag, line) = runs.first().map_or((1, 1), |run| (run.tag, run.line));
        let w = sp(frame.width());
        let h = sp(frame.height());

        let _ = writeln!(self.content, "{{{number}");
        let _ = writeln!(self.content, "[{tag},{line}:0,{h}:{w},{h},0");
        self.records += 1;

        for run in runs {
            let _ = writeln!(
                self.content,
                "({},{}:{},{}:{},{},{}\n)",
                run.tag,
                run.line,
                sp(run.origin.x),
                sp(run.origin.y),
                sp(run.width),
                sp(run.ascent),
                sp(run.descent)
            );
            self.records += 1;
        }

        let _ = writeln!(self.content, "]\n}}{number}");
    }

    fn collect(&mut self, frame: &Frame, ts: Transform, runs: &mut Vec<Run>) {
        for &(pos, ref item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    let ts = ts
                        .pre_concat(Transform::translate(pos.x, pos.y))
                        .pre_concat(group.transform);
                    self.collect(&group.frame, ts, runs);
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

                    let mut start = pos;
                    let mut x = pos.x;
                    let mut current: Option<Run> = None;
                    for glyph in &text.glyphs {
                        let advance = glyph.x_advance.at(text.size);
                        let loc = self.locate(glyph.span.0, glyph.span.1 as usize);

                        let same = matches!((&current, loc),
                            (Some(run), Some((tag, line))) if run.tag == tag && run.line == line);
                        if !same {
                            if let Some(run) = current.take() {
                                runs.push(run);
                            }
                            start = Point::new(x, pos.y);
                            current = loc.map(|(tag, line)| Run {
                                tag,
                                line,
                                origin: start.transform(ts),
                                width: Abs::zero(),
                                ascent,
                                descent,
                            });
                        }

                        x += advance;
                        if let Some(run) = &mut current {
                            run.width = x - start.x;
                        }
                    }
                    if let Some(run) = current {
                        runs.push(run);
                    }
                }
                _ => {}
            }
        }
    }

    /// Resolves a span to its file tag and 1-based line.
    fn locate(&mut self, span: typst::syntax::Span, offset: usize) -> Option<(usize, usize)> {
        let id = span.id()?;
        if !self.tags.contains_key(&id) {
            let entry = self.register(id);
            self.tags.insert(id, entry);
        }
        let (tag, source) = self.tags.get(&id)?.as_ref()?;

        let node = source.find(span)?;
        let line = source.lines().byte_to_line(node.offset() + offset)?;
        Some((*tag, line + 1))
    }

    fn register(&mut self, id: FileId) -> Option<(usize, Source)> {
        let source = self.world.source(id).ok()?;
        let path = self.world.path_for_id(id).ok()?;
        self.files
            .push(path.as_path().to_string_lossy().into_owned());
        Some((self.files.len(), source))
    }
}

fn sp(len: Abs) -> i64 {
    (len.to_pt() * SP_PER_PT).round() as i64
}
