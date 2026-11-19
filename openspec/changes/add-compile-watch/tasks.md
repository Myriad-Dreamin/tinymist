## 1. Command

- [x] 1.1 Add `--watch` to `CompileArgs` in `crates/tinymist-cli/src/cmd/compile.rs`.
- [x] 1.2 After resolving the universe, branch into `watch_main(universe, task)` when the flag is set; keep the one-shot path unchanged.
- [x] 1.3 In `watch_main`, start the project with `start_project`, export target `Html` for HTML tasks and `Paged` otherwise.
- [x] 1.4 On `Interrupt::Compiled`, print diagnostics; with errors print `compiled with errors`, otherwise hand the artifact to one export task, which runs `ExportTask::do_export` on the newest artifact and prints `compiled successfully` when it finishes or `export failed:` with the error.
- [x] 1.5 Add `--on-export <COMMAND>` (requires `--watch`); after a successful export run it through the shell for every written file with the path as last argument and `TINYMIST_OUTPUT`, before the status line; report failures on stderr.
- [x] 1.6 Pass the lock directory of `--save-lock` to every export.

## 2. Validate

- [x] 2.1 `cargo build --release -p tinymist-cli`; run `tinymist compile --watch main.typ out/main.pdf` on a multi-file document: the first compilation exports and prints `compiled successfully`.
- [x] 2.2 Append to an included file: a second compilation runs, the PDF is replaced, the status line is printed again.
- [x] 2.3 `cargo clippy -p tinymist-cli --all-targets`, `cargo fmt --check --all`.
- [x] 2.4 Add an e2e case that starts `compile --watch`, touches a dependency and expects two status lines, then terminates the process.
- [x] 2.5 On Unix, give the e2e case an `--on-export` hook script and expect it to record both exports.
- [x] 2.6 Add an e2e case with a slow hook and several quick edits: hooks do not overlap.
- [x] 2.7 Add an e2e case: `--watch --save-lock` writes the lock file of a run without `--watch`.
- [x] 2.8 Run `tinymist-cli` clippy with and without default features.

## 3. Documentation

- [x] 3.1 Document `--watch` and `--on-export` in `docs/tinymist/feature/cli.typ`.
