## Context

SyncTeX is the de-facto interface between TeX engines and PDF viewers. The format is documented in `synctex(5)` and in J. Laurens, "Direct and reverse synchronization with SyncTeX", TUGboat 29:3 (2008). The viewer side is largely one C library, `synctex_parser`, embedded in Okular, Zathura and SumatraPDF, among others. It reads a text file (plain or gzip) with a header naming the input files, a `Content:` section with one block per page and nested box records with coordinates, and a postamble. Every record carries a file tag and a source line. Forward search looks up all records with a given tag and line and shows the first; backward search finds the box enclosing a point and returns its tag and line.

Typst's `PagedDocument` holds, per page, a `Frame` of items. `FrameItem::Text` carries shaped glyphs, each with the `Span` of the source node and a byte offset into it, and the font metrics give ascent and descent. `FrameItem::Group` nests a frame under a transform. The `CompilerWorld` resolves a span's `FileId` to a `Source` (for the line) and to an absolute path.

## Goals / Non-Goals

**Goals:**
- Produce a file that `synctex_parser` reads without warnings, so every viewer that supports LaTeX works unchanged.
- Map every text run to the source line of its glyphs, across all files of a project (chapters, templates, packages).
- Keep the output opt-in and the cost confined to `--synctex` runs.

**Non-Goals:**
- Records for shapes, images or links; viewers fall back to the nearest text box.
- Column precision inside a line. SyncTeX forward search works on lines; backward search reports a column of -1 like pdfTeX does for most boxes.
- Supporting `--pages`; see decision 8.
- SyncTeX for the browser preview, which has its own jump protocol.
- A language server setting. `tinymist.exportPdf` users would benefit, but that is a separate configuration decision; see Open Questions.

## Decisions

### 1. Emit a subset of pdfTeX's file layout, with scaled points from the top-left page corner

The generator writes the header (`SyncTeX Version:1`, `Input:` lines, `Output:pdf`, `Magnification:1000`, `Unit:1`, offsets 0), then per page `{n`, one page-sized vbox `[`…`]`, one hbox `(`…`)` per text run, `}n`, and the postamble with the number of box records. Lengths are scaled points, `pt × 72.27 / 72 × 65536`, measured from the top-left corner with y downwards; this is what pdfTeX writes with zero offsets, so the parser's unit conversion applies unchanged.

pdfTeX writes more: `!` byte-offset anchors, which only speed up reading, records for glue, kerns and math, and a `Count` of all records. The generator writes none of these; `synctex_parser` does not need the anchors and does not check the count. A page without text runs gets `{n` and `}n` and no vbox, since any tag and line would be invented; the parser accepts this, finds nothing on that page, and keeps the page numbering of the pages after it.

Alternative considered:
- A reduced custom format read by a viewer plug-in. Rejected: the value of SyncTeX is that existing viewers already speak it.

### 2. One hbox per text run, split where the source line changes

A text run (`FrameItem::Text`) is one shaped segment of one line of output, but its glyphs may come from several source lines when a paragraph is wrapped differently in the source. The generator walks the glyphs, resolves each to (tag, line), and starts a new hbox whenever that pair changes. The hbox origin is the baseline start, its width the sum of advances, its height the font's ascent and its depth the font's descent at the text size.

A glyph's span names a syntax node, and its offset counts bytes into that node's text. Only `Text` and `MathText` nodes render their own source, so only for them is node start plus offset a source position, clamped to the node's end; any other node, such as the call of `lorem(50)` or a string expression, generated its text, and all its glyphs map to the node's start. This is the rule of `jump_from_click` in `typst-ide`. Adding the offset to other nodes would map generated text to the lines after its expression, or to no line once the offset passes the end of the file. Content that a function returns keeps the spans of the function's body, so its text maps there, not to the call. When a file first contributes a glyph, the generator indexes every node of its syntax tree by span in one traversal. `Source::find` would search from the root for each distinct span instead, which is quadratic in a file with many of them: with eight inline equations per paragraph it took 11 s for 2400 paragraphs.

Alternative considered:
- One hbox per glyph. Rejected: ten times the records for no gain in line precision.
- One hbox per run with the first glyph's line. Rejected: a wrapped paragraph would send forward search for the second source line to the wrong place or nowhere.

### 3. Map each run's box to page coordinates

Groups carry translations for most layout and full affine transforms for rotated or scaled content. The generator composes the transforms along the path to a text item and transforms the four corners of each run's local box. SyncTeX boxes are upright, so the record is their bounding box: x and width from the corners, the baseline start's y clamped into the box, height and depth the distances to its top and bottom. Scaled text gets its scaled extent; rotated text gets a box that also covers some space next to the glyphs. Backward search finds the box enclosing a point, so a box in the item's own scale would send clicks on scaled text to a neighbouring line. For the same reason, the generator cuts each box to the bounding box of the clips around it, intersected along the path, and drops a run with nothing left: text that a box clips away would otherwise catch clicks on the visible text over it.

### 4. Skip text inside outline entries

Entries of `outline()` render the heading's text with the heading's span, so forward search for a heading line would list the table of contents first and the viewer would show it instead of the heading. The generator tracks `Tag::Start`/`Tag::End` of `OutlineEntry` elements in the frame and emits no records between them. Text with a detached span has no source and gets no record. The text of a reference such as `@a` is of this kind; a click on it finds a neighbouring record, or nothing on a page without one.

Alternative considered:
- Keep all records and rely on the viewer choosing the best match. Rejected: `synctex_parser` returns matches in file order and Skim takes the first.

### 5. Absolute paths in `Input:` records

`synctex_parser` resolves relative input names against the PDF's directory, which for Typst projects is usually an output directory outside the source tree. Absolute paths, resolved through the world's path mapper, work from any output location and let the viewer open package files from the cache. The main file is included like every other source; there is no distinguished tag.

### 6. Generate in the export task, and write the file before the PDF

The SyncTeX file belongs to a specific PDF, so `ExportTask::do_export` keeps the paged document alongside the PDF bytes and writes `<output>.synctex.gz` with the same atomic write, before the PDF. A viewer that reloads when the PDF changes then reads the matching file, and if generating or writing the file fails, the command fails and the old PDF stays. If writing the PDF fails after that, the SyncTeX file is ahead of it until the next export. An export without the flag leaves an existing SyncTeX file in place, even though it no longer matches. The generator runs in the blocking pool like the PDF export itself. The compile command and the language server share this path, so an export task with `synctex: true` produces the same file wherever it runs. The language server's own tasks set it to false for now.

### 7. Compress with gzip

`pdflatex -synctex=1` writes `.synctex.gz`, and viewers read both that and plain `.synctex`. The text is highly repetitive: on a 107-page document with 43972 text runs, 2.1 MB shrink to 430 kB, about 13 % of the 3.3 MB PDF. `flate2` is a Rust gzip library that `tinymist-world` already depends on, so this adds no package to the build.

Alternative considered:
- Plain `.synctex` without the dependency. Rejected: five times the size next to every PDF, for one dependency edge.

### 8. Reject output the file cannot describe

The flag is meaningful only for a PDF of all pages. For other formats it would be ignored. With `--pages` the file's page numbers would follow the document and not the exported subset, so viewers would jump to wrong pages. Both are errors: clap rejects `--synctex --pages`, and `TaskCompileArgs::to_task` rejects non-PDF formats once the format is inferred from the output path.

Alternative considered:
- Filter and renumber pages for `--pages`. Rejected for now: more code for a mode that is rarely combined with live navigation, and it can be added later without breaking anyone.

## Risks / Trade-offs

- [Outline detection depends on frame tags] -> Tags are placed for every `Locatable` element in paged export; if a future Typst version drops them, records degrade to the pre-change behavior, nothing breaks.
- [Viewers assume one `Input:` per file and stable paths] -> Paths come from the world's resolver, the same source the language server uses for diagnostics.
- [Rotated text gets a bounding box larger than its glyphs] -> A click next to rotated text may resolve to it; acceptable for line lookup.
- [The span index covers whole files] -> A file that contributes one glyph is still indexed in full, in time linear in its size.
- [Clips are approximated by their bounding box] -> Text hidden by a rounded or rotated clip can keep a box in the clip's corners.
- [Generated text maps to the start of its expression] -> A multi-line expression maps all its text to its first line; it cannot map to anything more precise.

## Migration Plan

1. Add the `synctex` flag to the task model and CLI arguments with a false default.
2. Add the generator in `tinymist-task` behind the `pdf` feature.
3. Write the file from the export task when the flag is set.
4. Validate with `synctex view` and `synctex edit` from TeX Live against a multi-file document, then with a viewer.

No rollback concerns: without the flag the export path is unchanged.

## Open Questions

- Whether the language server should offer a setting, so that `tinymist.exportPdf` writes the file as well; the task model already carries the flag.
