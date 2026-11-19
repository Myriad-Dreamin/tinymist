## Why

`tinymist compile` exports once and exits. Users who want tinymist's export options while writing, such as the SyncTeX output of `add-synctex-export`, have to rerun the command by hand or wrap it in a file watcher; `typst watch` cannot be used because it does not know those options, and `tinymist preview` shows the document in a browser instead of writing a file. Tinymist already has a project compiler with a filesystem watcher that recompiles on changes, used by the language server, `tinymist preview` and `tinymist package docs --watch`.

## What Changes

- Add `--watch` to `tinymist compile`. The command compiles, exports, and repeats both whenever a file the document depends on changes, until it is interrupted.
- Print diagnostics after every compilation and a one-line status (`compiled successfully`, `compiled with errors`) that build scripts can match, as they do with `typst watch`.
- Export one version at a time and skip versions that are replaced meanwhile.
- Add `--on-export <COMMAND>` for watch mode: after every successful export the command runs through the system shell with the written file as its last argument and in `TINYMIST_OUTPUT`, so a build script can reload a viewer without parsing the output.

## Capabilities

### New Capabilities

- `compile-watch`: Defines the watch mode of `tinymist compile`: when it recompiles, what it exports, what it prints, and the command it runs after an export.

### Modified Capabilities

- None.

## Impact

- Affected code: `crates/tinymist-cli/src/cmd/compile.rs`.
- Docs: `docs/tinymist/feature/cli.typ`.
- Reuses `tinymist::tool::project::start_project` and `ExportTask::do_export`; no new dependencies.
- No change for runs without `--watch`.
