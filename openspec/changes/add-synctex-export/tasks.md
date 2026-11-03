## 1. Task Model and Arguments

- [x] 1.1 Add `synctex: bool` to `ExportPdfTask`, serialized only when true, default false.
- [x] 1.2 Add `--synctex` to `TaskCompileArgs` and pass it through `TaskCompileArgs::to_task`; `PdfExportArgs` is shared with `tinymist preview`, which writes no PDF.
- [x] 1.3 Set `synctex: false` at the remaining constructors of `ExportPdfTask`.
- [x] 1.4 Reject `--synctex` together with `--pages` and for non-PDF output.
- [x] 1.5 Pass `--synctex` through `tinymist generate-script`.

## 2. Generator

- [x] 2.1 Add `crates/tinymist-task/src/compute/synctex.rs` with `synctex_gz(world, doc) -> Bytes`, behind the `pdf` feature, and `flate2` as a dependency of `tinymist-task`.
- [x] 2.2 Walk page frames recursively, composing group transforms, and collect one run per (file, line) stretch of glyphs with baseline origin, width, ascent and descent.
- [x] 2.3 Resolve spans to file tags and 1-based lines through the world; assign tags in order of first appearance and record absolute paths.
- [x] 2.4 Skip text between the start and end tags of `OutlineEntry` elements.
- [x] 2.5 Write header, per-page blocks, postamble and record count in pdfTeX's layout, coordinates in scaled points from the top-left corner, gzip-compressed.

## 3. Export

- [x] 3.1 In `ExportTask::do_export`, keep the paged document when the task is a PDF export with `synctex` set, and write `<output>.synctex.gz` with `write_atomic` after the PDF.

## 4. Validate

- [x] 4.1 `cargo build --release -p tinymist-cli`, then compile a multi-file document with `--synctex` and confirm `<output>.synctex.gz` appears next to the PDF.
- [x] 4.2 `synctex view -i <line>:1:<absolute source path> -o <pdf>` returns the page and position of that line; `synctex edit -o <page>:<x>:<y>:<pdf>` returns the source file and line under that point.
- [x] 4.3 Forward search for a heading line lands on the heading, not on the table of contents.
- [x] 4.4 A PDF viewer (Skim): forward search shows the line, Shift-Command-click opens the source line in the editor.
- [x] 4.5 `cargo test -p tinymist-task`, `cargo clippy -p tinymist-task -p tinymist --all-targets`, `cargo fmt --check --all`.
- [x] 4.6 Add e2e tests for the `Input:` records of a two-file document, for outline entries, source line splits and group transforms, for rejected flag combinations, and for `generate-script`.

## 5. Documentation

- [x] 5.1 Document `--synctex` in `docs/tinymist/feature/cli.typ` and `docs/tinymist/feature/preview.typ`.
