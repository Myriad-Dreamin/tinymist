## Why

SyncTeX is how LaTeX editors and PDF viewers link a PDF to its sources. The compiler writes a file next to the PDF that lists the boxes on each page with the source file and line they came from. Viewers use it for forward search (show the PDF position of the editor's line) and backward search (click the PDF to open that line in the editor). Evince, Okular, Skim, SumatraPDF, Zathura and others read this file, so any compiler that writes it gets this navigation without viewer changes.

Tinymist documents two ways to preview a document: the web preview, with bidirectional navigation, and PDF preview in the user's own viewer, without any navigation. SyncTeX closes that gap. It complements the web preview:

- For SyncTeX: it shows the final PDF in a native viewer, and it works from any editor that can run a command, without a language server.
- Against it: it jumps by line instead of by position, covers only text, and exports the whole PDF on each compilation.

Typst closed its SyncTeX request (typst/typst#289), pointing to its `ide` functions as what a language server would call. Those functions answer one position at a time; a SyncTeX file lists all positions in advance. Tinymist already has what that needs, the paged document with spans on every glyph and a world that maps spans to files and lines, and it is the tool that connects editors and viewers. The generator reads only public Typst data and can be removed if Typst ever writes the file itself.

#2704 asks for forward search to Skim through a new LSP command, which every editor and viewer pair would have to script. SyncTeX serves that case without per-viewer code, in both directions.

## What Changes

- Add `--synctex` to `tinymist compile`. After a successful PDF export the compiler writes `<output>.synctex.gz` next to the PDF.
- Reject `--synctex` for non-PDF output and together with `--pages`, where it could only be ignored or wrong.
- Add a `synctex` flag to the PDF export task so the lock file and `tinymist generate-script` keep the option.
- Add a SyncTeX generator to `tinymist-task` that walks the page frames and emits one hbox record per text run, tagged with the source file and line of its glyphs, in the format of pdfTeX.
- Document the option in the CLI and preview feature pages.

## Capabilities

### New Capabilities

- `synctex-export`: Defines the SyncTeX file written next to an exported PDF and how PDF positions map to source lines.

### Modified Capabilities

- None.

## Impact

- Affected code: `crates/tinymist-task/src/compute/synctex.rs` (new), `crates/tinymist-task/src/compute.rs`, `crates/tinymist-task/src/model.rs`, `crates/tinymist-project/src/args.rs`, `crates/tinymist/src/task/export.rs`, `crates/tinymist/src/cmd/export.rs`, `crates/tinymist/src/config.rs`, `crates/tinymist-cli/src/cmd/generate_script.rs`.
- `tinymist-task` gains a dependency on `flate2`, a gzip library that `tinymist-world` already uses, so `Cargo.lock` adds no package. The e2e tests use it to read the file back.
- The export task model gains an optional boolean field; serialized tasks without it keep their meaning.
- No change for users who do not pass `--synctex`.
