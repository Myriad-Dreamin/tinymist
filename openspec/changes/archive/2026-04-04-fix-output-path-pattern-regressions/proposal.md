## Why

Issue `#2400` reports that `tinymist.outputPath` behaves inconsistently in two common cases:

- When `$name` expands to a filename stem that already contains dots, the final export path is built with `PathBuf::with_extension`, which truncates everything after the last dot of the substituted stem. That turns filenames like `Chapter 1.1.typ` into `Chapter 1.pdf` instead of `Chapter 1.1.pdf`.
- When a file lives at the workspace root, substituting `$dir/$name` can leave a leading slash from the empty `$dir`, turning an intended workspace-relative path into an absolute filesystem-root path such as `/Chapter 1.pdf`.

Those regressions make `tinymist.outputPath` unreliable for chapter-style filenames and break the documented expectation that leaving `outputPath` empty behaves like using `$dir/$name`.

## What Changes

- Preserve the full Typst source stem when applying the artifact extension to paths derived from `$name`.
- Treat an empty `$dir` substitution as the workspace-relative current directory instead of a literal root slash.
- Align explicit `"$dir/$name"` resolution with the existing empty `outputPath` default so both configurations export to the same artifact location.
- Add regression coverage for multi-dot filenames, repeated-dot stems, and workspace-root files.
- Review the `tinymist.outputPath` documentation source so its examples and default-behavior wording match the fixed implementation.

## Capabilities

### New Capabilities

- `output-path-patterns`: Resolve `tinymist.outputPath` patterns without truncating multi-dot stems or escaping the workspace root when `$dir` is empty.

### Modified Capabilities

- None.

## Impact

- `crates/tinymist-task/src/primitives.rs`
- `crates/tinymist/src/task/export.rs`
- Output-path related tests in `crates/tinymist-task` and `crates/tinymist`
- `docs/tinymist/feature/preview.typ` or other non-generated documentation sources if wording changes are needed
