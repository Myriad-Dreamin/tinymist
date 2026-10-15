# Editor Integration Scenario: tinymist × Helix on Windows

Same scope as the Linux scenario (`../helix-linux/TASK.md`), executed on Windows with
the portability constraints a Windows user faces: a different configuration path, a
native terminal multiplexer, and atomic-save behavior worth checking explicitly.

## Purpose

Exercise the documented setup path and the core language features inside Helix on
Windows, driven non-interactively, to catch documentation drift and server regressions
specific to this platform.

## Environment

- Platform: Windows.
- A recent Helix release (`hx --version` reports the installed version). Unpack `hx`
  together with the `runtime/` directory; moving `hx` alone loses the runtime.
- tinymist stable release binary, reachable on `PATH` (verify with `where tinymist`).
- Configuration: `%APPDATA%\helix\languages.toml` (user-level), or the project-level
  `.helix\languages.toml` form — both are accepted. A portable form using
  `hx -c <config> --log <file>` with the config and its `languages.toml` kept together
  is also available for zero-AppData setups.
- Terminal multiplexer: psmux (the Windows-native tmux reimplementation) to drive the
  editor and capture panes; any scripted driver with equivalent capabilities works.
- A scratch project directory containing `main.typ`.

## Steps

1. **Configure the server**: write the minimal snippet from the README into
   `%APPDATA%\helix\languages.toml` (or a project-level `.helix\languages.toml`).
   Expected: the file parses without errors.
2. **Open the project**: drive `hx main.typ` in the multiplexer, wait a few seconds.
   Expected: `tinymist` appears as a child process of `hx`.
3. **Diagnostics on open**: put a semantic error in `main.typ`.
   Expected: gutter marking and a status-bar diagnostics count, as on Linux.
4. **Completion**: type `#set ` in insert mode.
   Expected: the completion menu appears with typed entries.
5. **Hover**: hover over `page` (`Space k`).
   Expected: the full signature popup.
6. **Save awareness with atomic-save**: set `exportPdf = "onSave"`, then save with
   Helix's atomic-save enabled.
   Expected: the export fires on save — no `atomic-save = false` workaround is
   required.
7. **Background preview**: set `preview.background.enabled = true`, reopen.
   Expected: the preview server listens on the configured `--data-plane-host` address
   (default `127.0.0.1:23635`); `GET /` returns 200.
8. **Syntax-only mode**: set `syntaxOnly = "enable"`, reopen.
   Expected: semantic diagnostics and export stop; syntax-level features keep working.
9. **Stability stress**: run a scripted edit loop of a few hundred compiles.
   Expected: zero panics or crashes of the language server.

## Coverage notes

- Raw evidence for a run (LSP mirror logs, pane captures, reproduction scripts) stays
  in the QA workspace and is not committed to the repository.
- Known caveats — including platform-specific ones — and their current status are
  maintained in the QA scenario status page in that workspace; a step whose expectation
  fails is not by itself a scenario failure — consult the status page before judging.

## Artifacts

Only this `TASK.md` is committed to the repository. Anything captured while running the
scenario belongs to the QA workspace; the scenario status page — execution history,
known caveats, and their current state — is maintained there as well.
