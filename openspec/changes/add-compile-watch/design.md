## Context

`compile_main` resolves the input to an `LspUniverse`, takes one snapshot, compiles it and hands the artifact to `ExportTask::do_export`. The project compiler in `tinymist-project` wraps the same universe in a `ProjectCompiler` that watches dependencies through `notify`, recompiles on change and delivers `Interrupt::Compiled(artifact)` to a handler. `tinymist::tool::project::start_project` sets that up for standalone commands; `tinymist package docs --watch` is the existing consumer.

## Goals / Non-Goals

**Goals:**
- Recompile and re-export on every change of a depended file, with the same task (output path, format, PDF options) as the one-shot run.
- Print the same diagnostics as the one-shot run after each compilation.
- Print a status line a shell loop can match, compatible with what scripts already match for `typst watch`.

**Non-Goals:**
- Watching files the document does not depend on, or glob patterns.
- Serving the preview; that is `tinymist preview`.

## Decisions

### 1. Branch into `start_project` after task resolution

`compile_main` resolves input, task and universe exactly as before. With `--watch` it hands the universe to `start_project` instead of taking a snapshot, so argument handling, lock-file writing and output-path resolution stay in one place. The export target is `Html` for an HTML task and `Paged` otherwise, so the compiler produces the document kind the task needs.

Alternative considered:
- A loop around the one-shot path with a hand-rolled `notify` watcher. Rejected: duplicates dependency tracking that `ProjectCompiler` already does, including package files and fonts.

### 2. Export the newest artifact on one separate task

On `Interrupt::Compiled`, the handler prints diagnostics and, when the artifact has no errors, stores it in a `tokio::sync::watch` channel. The handler then passes the interrupt on to the default handler so the compiler's own bookkeeping runs. One task reads the channel and runs `ExportTask::do_export`, so the compiler loop stays responsive to the next change.

The channel holds only the newest artifact. Exports therefore never overlap, an older export cannot replace a newer one, a PDF and files derived from it come from the same compilation, and a slow export or `--on-export` command builds up no queue.

The lock directory of `--save-lock` goes to every export, so the lock file equals that of a one-shot run.

Alternative considered:
- One spawned task per compilation, as in `tinymist package docs --watch`. Rejected: exports then overlap and can finish in any order.
- Configure the project compiler's built-in export task through `ExportUserConfig`. Rejected: that path is wired to language server settings and `TaskWhen`; the CLI task already is a `ProjectTask`, and `do_export` takes it directly.

### 3. Status lines after export, on stderr

`compiled successfully` is printed after the export finished, not after compilation, so a script that reloads a viewer on that line sees the finished files. `compiled with errors` is printed when diagnostics contain errors and no export happens; export failures print `export failed:` with the error. Diagnostics and status go to stderr like in the one-shot run.

### 4. `--on-export` runs through the shell, after the export and before the status line

Build scripts that reload a PDF viewer (`latexmk -pvc` style) otherwise have to pipe stderr through a loop that matches the status line. `--on-export <COMMAND>` takes one string, runs it as `sh -c '<COMMAND> "$1"' sh <path>` (`cmd /S /C` with a raw argument on Windows, because `cmd` does not understand the quote escaping of `Command::arg`) with `TINYMIST_OUTPUT` set, once per written file, and only after a successful export. Going through the shell lets the user pass a command with its own arguments without a parsing crate. The command runs on the blocking pool and before `compiled successfully` is printed, so a script that waits for the status line sees the hook's effect; a failing or missing command is reported on stderr and watching continues. The option requires `--watch`; clap rejects it otherwise.

Alternative considered:
- A `TaskWhen`-style hook in the task model, usable from the language server too. Rejected for now: the lock file would carry a shell command, which is a different trust level from compile options.

## Risks / Trade-offs

- [A status line can follow a newer `compiled with errors`] -> The export of the last good compilation may still be running when the next compilation fails. The output is still that good version.
- [An entry file that does not exist yet is not an error at start] -> The run reports `compiled with errors` and exports once the file appears, unlike the one-shot run, which exits.
- [The handler sees compilations of every project instance] -> Standalone projects have one instance; if dedicated instances are added later, the handler should filter by `artifact.graph.snap.id`.
- [The hook string is interpolated into a shell command] -> It comes from the command line of the same user, like `--input`; the file path is passed as a positional parameter, not interpolated.
- [`start_project` is marked as extracted from `tinymist preview` and not yet abstracted] -> `compile --watch` is a second consumer next to `package docs --watch`; a later refactor has three call sites to unify.

## Migration Plan

1. Add the flag and the `watch_main` branch.
2. Verify first compilation, recompilation after a source change, and the status lines.

No rollback concerns: the one-shot path is untouched.

## Open Questions

- Whether a `--watch` run should exit non-zero on the first failed compilation when stdin is not a terminal, for CI use. Not needed for the editor workflow this targets.
