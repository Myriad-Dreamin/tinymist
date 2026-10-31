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

### 1. Emit pdfTeX's file layout, with scaled points from the top-left page corner

The generator writes the header (`SyncTeX Version:1`, `Input:` lines, `Output:pdf`, `Magnification:1000`, `Unit:1`, offsets 0), then per page `{n`, one page-sized vbox `[`…`]`, one hbox `(`…`)` per text run, `}n`, and the postamble with a record count. Lengths are scaled points, `pt × 72.27 / 72 × 65536`, measured from the top-left corner with y downwards; this is what pdfTeX writes with zero offsets, so the parser's unit conversion applies unchanged.

Alternative considered:
- A reduced custom format read by a viewer plug-in. Rejected: the value of SyncTeX is that existing viewers already speak it.

### 2. One hbox per text run, split where the source line changes

A text run (`FrameItem::Text`) is one shaped segment of one line of output, but its glyphs may come from several source lines when a paragraph is wrapped differently in the source. The generator walks the glyphs, resolves each to (tag, line), and starts a new hbox whenever that pair changes. The hbox origin is the baseline start, its width the sum of advances, its height the font's ascent and its depth the font's descent at the text size.

Alternative considered:
- One hbox per glyph. Rejected: ten times the records for no gain in line precision.
- One hbox per run with the first glyph's line. Rejected: a wrapped paragraph would send forward search for the second source line to the wrong place or nowhere.

### 3. Apply group transforms to the origin

Groups carry translations for most layout and full affine transforms for rotated or scaled content. The generator composes the transforms along the path to a text item and transforms the baseline origin; width, height and depth stay in the item's own scale. Rotated text therefore gets a correct anchor and an approximate extent, which is enough for line lookup.

### 4. Skip text inside outline entries

Entries of `outline()` render the heading's text with the heading's span, so forward search for a heading line would list the table of contents first and the viewer would show it instead of the heading. The generator tracks `Tag::Start`/`Tag::End` of `OutlineEntry` elements in the frame and emits no records between them. Index entries and cross references are not affected: their text carries the span of the `ref` or `index` call site, which is where a user expects to land.

Alternative considered:
- Keep all records and rely on the viewer choosing the best match. Rejected: `synctex_parser` returns matches in file order and Skim takes the first.

### 5. Absolute paths in `Input:` records

`synctex_parser` resolves relative input names against the PDF's directory, which for Typst projects is usually an output directory outside the source tree. Absolute paths, resolved through the world's path mapper, work from any output location and let the viewer open package files from the cache. The main file is included like every other source; there is no distinguished tag.

### 6. Generate in the export task after the PDF bytes are written

The SyncTeX file belongs to a specific PDF, so `ExportTask::do_export` keeps the paged document alongside the PDF bytes and writes `<output>.synctex.gz` with the same atomic write after the PDF. The generator runs in the blocking pool like the PDF export itself. The compile command and the language server share this path, so an export task with `synctex: true` produces the same file wherever it runs. The language server's own tasks set it to false for now.

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
- [Rotated or scaled text extents are approximate] -> Line lookup needs only the anchor; acceptable.

## Migration Plan

1. Add the `synctex` flag to the task model and CLI arguments with a false default.
2. Add the generator in `tinymist-task` behind the `pdf` feature.
3. Write the file from the export task when the flag is set.
4. Validate with `synctex view` and `synctex edit` from TeX Live against a multi-file document, then with a viewer.

No rollback concerns: without the flag the export path is unchanged.

## Open Questions

- Whether the language server should offer a setting, so that `tinymist.exportPdf` writes the file as well; the task model already carries the flag.
