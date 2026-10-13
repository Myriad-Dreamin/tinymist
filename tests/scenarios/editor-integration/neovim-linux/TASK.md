# Editor Integration Scenario: tinymist × Neovim on Linux

Verifies that tinymist works correctly under the Neovim editor on Linux, by walking the
`editors/neovim/README.md` setup guide end to end as a user would.

## Purpose

Exercise the documented setup paths and the core language features inside Neovim, driven
non-interactively, to catch documentation drift and server regressions before release.

## Environment

- Platform: Linux.
- A recent Neovim release (`nvim --version` reports the installed version).
- The nvim-lspconfig plugin on `'runtimepath'` for the lspconfig route; the builtin
  `vim.lsp.config` route needs no plugin.
- tinymist stable release binary, reachable on `PATH` (verify with
  `command -v tinymist`).
- Headless Neovim with Lua probes drives the checks non-interactively
  (`nvim --headless -u <init.lua> <file.typ> -c "luafile <probe.lua>"`); protocol-level
  assertions go through `client:request_sync`. tmux (`send-keys` + `capture-pane`)
  drives interactive checks (completion menus, floating windows); start the tmux server
  with a `PATH` that includes the tinymist binary — a detached server started without it
  fails to launch language servers.
- A scratch project directory containing `main.typ`.

## Steps

1. **Configure the server (lspconfig route)**: write the minimal snippet from the README
   — `require("lspconfig")["tinymist"].setup { settings = { ... } }` — into the
   user-level `~/.config/nvim/init.lua`, or pass the same configuration with an init
   file (`-u <init.lua>`).
   Expected: the configuration loads without errors.
2. **Configure the server (builtin route)**: write the builtin snippet from the README —
   `vim.lsp.config["tinymist"] = { cmd = { "tinymist" }, filetypes = { "typst" },
   settings = { ... } }` — and start the server with `vim.lsp.enable("tinymist")`.
   Defining the configuration alone does not start a client; the enable call does.
   Expected: opening a `typst` buffer attaches the server.
3. **Open the project**: open `main.typ` (headless with a probe, or interactively), wait
   a few seconds.
   Expected: a tinymist client attaches to the buffer and `tinymist` runs as a child
   process of nvim; `:checkhealth vim.lsp` reports the client with its resolved command
   and settings; the client log under `stdpath('state')` collects the server's stderr.
   In a scratch directory without a project marker the client still attaches — the
   server resolves the single file's directory as the root (a non-null root can be
   pinned with the `rootPath` setting or lspconfig's `root_dir`).
4. **Diagnostics on open**: put a semantic error in `main.typ` (e.g. a reference to an
   unknown label, `@doesnotexist`).
   Expected: the diagnostic reaches `vim.diagnostic` without extra configuration; the
   gutter marks the line and the status line shows a diagnostics count.
5. **Completion**: in insert mode, type `#set ` and trigger omni-completion
   (`CTRL-X CTRL-O`; the lspconfig route wires `omnifunc`); at the protocol level,
   request completion right after `#se`.
   Expected: the menu (or the protocol response) lists typed entries — functions and
   snippets such as `set rule`; a longer prefix narrows the list to relevant entries.
6. **Hover**: hover over `page` (`K`, or `vim.lsp.buf.hover()`).
   Expected: the popup shows the full signature with parameters and documentation.
7. **Save awareness**: set `exportPdf = "onSave"` and save with `:w`; set
   `exportPdf = "onType"` and make an edit.
   Expected: the PDF appears at the configured `outputPath` — after the save event with
   `"onSave"`, after an edit (unsaved edits included) with `"onType"`.
8. **Formatting**: set the formatter settings from the README (`formatterMode =
   "typstyle"` or `"typstfmt"`, with `formatterPrintWidth`, `formatterProseWrap`,
   `formatterIndentSize` as desired). The formatting capability is delivered by dynamic
   registration: after the client attaches, wait for the `client/registerCapability`
   round trip to complete (for example, `client.supports_method("textDocument/formatting")`)
   before formatting.
   Expected: once the client reports the capability, `vim.lsp.buf.format()` and `gq`
   both produce the server-side formatter's output. Before the registration completes,
   `vim.lsp.buf.format()` reports no matching clients and `gq` falls back to Neovim's
   built-in text re-flowing, which does not preserve Typst code — confirm the
   capability before judging a `gq` result.
9. **Default preview command**: with the background preview disabled, set
   `preview.browsing.args` in the server `settings` to an explicit `--data-plane-host`
   (the default arguments pick an ephemeral port and include `--open`, which launches
   a browser — override them for non-interactive runs), then execute the
   `tinymist.startDefaultPreview` command through the workspace command route
   (`client:exec_cmd`).
   Expected: the command starts a browsing preview; its response reports the data plane
   port, and `GET /` over HTTP returns 200.
10. **Background preview**: enable `preview.background.enabled = true` and deliver it
    through `initializationOptions` — the server reads this setting only when it starts,
    so a workspace `settings` update alone does not start the preview — then restart the
    client and reopen the file.
    Expected: the preview server listens on the configured `--data-plane-host` address
    (default `127.0.0.1:23635`); `GET /` over HTTP returns 200; subsequent edits are
    reflected in the preview served there, and the `tinymist.startDefaultPreview`
    command defers to this primary preview instead of starting a second data plane.
11. **Syntax-only mode**: set `syntaxOnly = "enable"` in the server config, reopen.
    Expected: semantic diagnostics and export stop; syntax checking keeps reporting
    parse errors, completion keeps responding with the syntax-level set, formatting
    keeps responding, and hover and go-to-definition keep answering — syntax-level
    analysis still runs; full-document compilation is what stops.
12. **Multiple-files project**: create `main.typ` including `sub.typ`, with a numbered
    heading carrying a label, and reference that label from `sub.typ`; open `sub.typ`.
    Expected: the label error appears on open (single-file resolution does not see the
    main file); executing the `tinymist.pinMain` command with the main file's path
    (`client:exec_cmd`) clears the diagnostics immediately, without an edit to trigger
    the update; unpinning — the same command with `vim.v.null`, not `nil` — makes the
    error return; the pin is not remembered across sessions.
13. **Label completion and definition**: in the pinned project, request completion at a
    reference position right after `@`, and go-to-definition on a resolved `@label`
    reference.
    Expected: completion offers the labels defined in the project (including the one in
    the main file); go-to-definition jumps to the label. The
    `tinymist.getWorkspaceLabels` workspace command returns the label set with
    positions, a cross-check that the server holds the data.

## Coverage notes

- Raw evidence for a run (LSP mirror logs, probe scripts, pane captures, environment
  notes) stays in the QA workspace and is not committed to the repository.
- Known caveats and their current status are maintained in the QA scenario status page
  in that workspace; a step whose expectation fails is not by itself a scenario
  failure — consult the status page before judging.
- The README also documents a Coc.nvim configuration route and a third-party web-preview
  plugin (`typst-preview.nvim`); both bring their own runtimes and are outside this
  scenario's scope — the two native configuration routes and the server-side preview
  commands cover that ground here, and the PDF-reader route reduces to the export step
  plus an external viewer.
- The repository ships its own Neovim plugin under `editors/neovim/` with a
  container-based end-to-end suite (see `editors/neovim/CONTRIBUTING.md`); this scenario
  complements that suite against a stock, README-only setup.

## Artifacts

Only this `TASK.md` is committed to the repository. Anything captured while running the
scenario (captures, mirror logs, screenshots) belongs to the QA workspace; the scenario
status page — execution history, known caveats, and their current state — is maintained
there as well.
