# Editor Integration Scenario: tinymist × Helix on Linux

Verifies that tinymist works correctly under the Helix editor on Linux, by walking the
`editors/helix/README.md` setup guide end to end as a user would.

## Purpose

Exercise the documented setup path and the core language features inside Helix, driven
non-interactively, to catch documentation drift and server regressions before release.

## Environment

- Platform: Linux (validated on a Debian-based distribution).
- Helix 25.07.1 (`hx --version` reports `helix 25.07.1 (...)`). Unpack `hx` together
  with the `runtime/` directory from the release archive — `hx` locates its runtime
  relative to its own path, so moving `hx` alone breaks it.
- tinymist stable release binary, reachable on `PATH` (verify with `tinymist -V`).
- tmux to drive the editor non-interactively (`send-keys` + `capture-pane`). Start the
  tmux server with a `PATH` that includes the tinymist binary — a detached server
  started without it fails to launch language servers.
- A scratch project directory containing `main.typ`.

## Steps

1. **Configure the server**: write the minimal snippet from the README into the
   user-level `~/.config/helix/languages.toml` (or a project-level
   `.helix/languages.toml`).
   Expected: the file parses without errors.
2. **Open the project**: `tmux new-session -d -s qa 'hx main.typ'`, wait a few seconds.
   Expected: `tinymist` appears as a child process of `hx`.
3. **Diagnostics on open**: put a semantic error in `main.typ` (e.g. an unknown
   function call).
   Expected: the gutter marks the line and the status bar shows a diagnostics count;
   hover reveals the message text (inline display requires the `inline-diagnostics`
   option in `config.toml`).
4. **Completion**: in insert mode, type `#set `.
   Expected: the completion menu appears with typed entries (functions and snippets).
5. **Hover**: move the cursor onto `page` and press `Space k`.
   Expected: the popup shows the full signature with parameters.
6. **Save awareness**: set `exportPdf = "onSave"` (or `"onType"`) in the server config,
   then save with `:w`.
   Expected: the PDF appears at the configured `outputPath`; editor-triggered export
   fires without any `atomic-save` workaround.
7. **Background preview**: set `preview.background.enabled = true`, reopen the file.
   Expected: the preview server listens on `127.0.0.1:23635`; `GET /` over HTTP returns
   200; subsequent edits are reflected in the viewer.
8. **Syntax-only mode**: set `syntaxOnly = "enable"` in the server config, reopen.
   Expected: semantic diagnostics and export stop; syntax checking, syntax-level
   completion, and formatting keep responding.

## Coverage notes

- Every step above was executed and captured during the 2026-10 QA probe. Raw evidence
  (LSP mirror logs, pane captures, environment notes) stays in the organization's QA
  workspace and is not committed to the repository.
- Known platform-independent caveats — the entry file may not compile on the first open
  with `typstExtraArgs` configured (first edit or save activates it), and label
  completion/`gd` on `@label` references currently returns nothing — are tracked as
  separate proposals; do not mark this scenario failed on those without checking the
  tracker first.

## Artifacts

Only this `TASK.md` is committed to the repository. Anything captured while running the
scenario (captures, mirror logs, screenshots) belongs to the QA workspace.
