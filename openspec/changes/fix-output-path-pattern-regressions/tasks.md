## 1. Fix output-path resolution semantics

- [ ] 1.1 Update `tinymist.outputPath` substitution so an empty `$dir` does not inject a filesystem-root separator for workspace-root files.
- [ ] 1.2 Change export-path extension handling so paths derived from `$name` preserve every stem segment before the trailing `.typ`.
- [ ] 1.3 Ensure leaving `tinymist.outputPath` empty and explicitly setting it to `$dir/$name` resolve to the same artifact target.

## 2. Add regression coverage

- [ ] 2.1 Extend `PathPattern`-level tests for workspace-root files, nested files, and multi-dot source names.
- [ ] 2.2 Add export-path tests covering PDF output for `Chapter 1.1.typ`, `Chapter 1.1.1.typ`, and `test....typ` with `$root/$dir/$name`.
- [ ] 2.3 Add a regression test showing that explicit `$dir/$name` at the workspace root exports beside the source file instead of attempting to write to `/`.

## 3. Validate user-facing behavior

- [ ] 3.1 Review and update the `tinymist.outputPath` documentation source if its wording still diverges from the fixed behavior.
- [ ] 3.2 Run focused tests for output-path substitution and export-path preparation.
