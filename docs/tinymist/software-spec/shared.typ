
#import "mod.typ": *

#let is-vscode = state("config:is-vscode", false)

#let html-link(dest) = if is-md-target {
  cross-link(dest, [HTML])
} else {
  [HTML]
}

#let md-link(dest) = if is-md-target {
  [Markdown]
} else {
  github-link(dest, [Markdown])
}

// #context if is-vscode.get() {
//   html-link("/config/vscode.typ")
//   [ | ]
//   md-link("/editors/vscode/Configuration.md")
// } else {
//   html-link("/config/neovim.typ")
//   [ | ]
//   md-link("/editors/neovim/Configuration.md")
// }

#let md-label(it) = {
  let it = lower(it).replace(regex("[^a-zA-Z0-9]"), "-")
  it.replace(regex("-{2,}"), "-").replace(regex("^-|-$"), "")
}

#let md-spec(body) = {
  show heading: it => if it.has("label") {
    it
  } else [
    #it#label(plain-text(it))
  ]
  show emph: it => if is-md-target {
    box(html.elem("m1verbatim", attrs: (
      src: {
        "[_" + it.body.text + "_](#"
        md-label(it.body.text)
        ")"
      },
    )))
  } else {
    it
  }

  body
}

#let vscode-spec = md-spec[
  This document describes the software specification of the VS Code extension for Tinymist.

  = Overview

  There are following user interfaces for Tinymist:
  - In the activity bar, there is a Tinymist icon that show up the _Tinymist Views_.
  - In the status bar, there is a _Document Status Indicator_ and a general _Problem Indicator_.
  - You can press `Ctrl+Shift+P` to open the command palette and type `Typst` to see all _Typst-Related Commands_.
  - You can also perform some contextual actions in the editor, such as _Drag and Drop_ a file to the editor, or `Right Click` in the editor to see the _Context Menu_.
  - There are tools in the _Tool View_ that you can use to automate typst tasks.
  - You can interact with the _Tinymist LSP_ under the state model _Entry File_, _Focus State_, _Pin State_, and _Preview State_.
  - You can configure the extension and the language server by the _Typst-Specific VS Code Configurations_.

  = Tinymist Views

  There are following views in the _Tinymist Views_:
  - _Symbol View_
  - _DevKit View_
  - _Tool View_
  - _Package View_
  - _Content View_
  - _Label View_
  - _Outline View_

  == Symbol View

  There are following actions that you can perform in the _Symbol View_:
  - Search Symbol by Names: You can search for a symbol by its name. The search is case-insensitive and supports partial matching.
  - Search Symbol by Handwritten Strokes: You can draw a symbol and search for it. The stroke search is powered by _Detypify_ with handwriting recognition running inside the view's webview.
  - The symbols are grouped by categories in the view.
  - Click to paste: You can click on a symbol to paste it to the editor.

  == DevKit View

  The _DevKit View_ shows development entries of the extension, for example, running the preview in development mode. The view is hidden by default: it only shows up when the extension is running in development mode or the `tinymist.devKit` setting is `enable`. This setting is not listed in the contributed settings, so you need to edit the settings JSON file to enable it.

  == Tool View

  There are following tools that you can use in the _Tool View_:
  - _Template Gallery_: You can use this tool to show the template gallery.
  - _Document Summary_: You can use this tool to show the document summary.
  - _Symbol View_: You can open the _Symbol View_ to the side.
  - _Font View_: You can use this tool to show the font view.
  - _Profiling_: You can use this tool to profile the current file.
  - _Profiling Server_: You can use this tool to profile the language server.

  == Package View

  There are following actions that you can perform in the _Package View_:
  - Create a new local package: You can create a new local package by clicking the `Create Local Package` button in the `commands` group.
  - Open a local package: You can open a local package by clicking the `Open Local Package` button in the `commands` group.
  - Get a tree view of the package: You can view packages in `@preview` and `@local` namespaces in tree.
  - Check documentation of a package: You can check the documentation of a package by clicking the _Documentation_ button associated with the package in the tree view.
  - Open a package in the editor: You can open a package in the editor by clicking the `Open` button associated with the package in the tree view.
  - Open a package in the file explorer: You can open a package in the file explorer by clicking the `Reveal in File Explorer` button associated with the package in the tree view.
  - Check exported symbols of a package: You can check the exported symbols of a package by expanding the `symbols` node in the tree view.

  == Content View

  You can get a thumbnail of the current document in the _Content View_ if _Preview State_ is enabled.

  == Label View

  You can find all syntactic labels in the workspaces in the _Label View_. The labels are grouped by prefix. You can click the `Scan workspace and collect all labels again` button in the view's title to refresh the labels.

  == Outline View

  You can get the document outline in the _Outline View_ if _Preview State_ is enabled. The outline is strictly same as the outline that will show up in the PDF document. This is different from the LSP's syntax outline which shows language syntax structure in the current opened document.

  = Tools

  == Template Gallery

  The _Template Gallery_ connects to the typst universe and shows the templates that are available in the universe.
  - You can search for a template by its name.
  - You can filter templates by their categories.
  - You can create a project from a template by clicking the `Creates Project` (+) button.
  - You can favorite a template by clicking the `Favorite` (♥) button. The favorite templates will show up when you initializes a template by command `tinymist` in place in to the current opened text document.

  == Document Summary

  The _Document Summary_ shows the summary of the current document. It includes:
  - The fonts and related information used by the document. The related information includes:
    - The font family, size, and style.
    - The occurrences of the glyphs (informally characters) in the document.
  - The arguments used to compile the document.

  - The resources used by the document.

  The summary is only available when the current document is a valid primary file that compiles successfully. For a document that fails to compile, the summary cannot be retrieved.

  == Font View

  The _Font View_ shows all of the fonts and related information recognized by the LSP server. The related information includes:
  - The font family, size, and style.
  - The font file name and path.

  There are following actions that you can perform in the _Font View_:
  - Click `Show Number`: You can click the `Show Number` button to show or hide accurate number of font weights.
  - Click `Copy`: You can click the `Copy` button to copy the typst family name of the font to the clipboard.
  - Click `Paste String`: You can click the `Paste String` button to paste the typst family name of the font directly to the editor.
  - Click `#set`: You can click the `#set` button to generate and paste a set rule to the editor.

  == Profiling

  The _Profiling_ tool allows you to profile the current file. It shows the time taken to compile the document by flame graph.

  == Profiling Server

  The _Profiling Server_ tool allows you to profile the language server. When you click the `Profiling Server` button in the tool view, the server will start profiling and you can see the profiling results in the _Profiling_ tool. You can stop profiling by clicking the `Stop Profiling` button.

  = Document Status Indicator

  In the status bar, the _Document Status Indicator_ shows the compilation status of the current document. It is controlled by the `tinymist.compileStatus` setting:
  - When the document is compiling, the indicator shows a spinning icon.
  - When the document compiles successfully, the indicator shows a normal status.
  - When the document fails to compile, the indicator is highlighted with the error background color.

  The text of the indicator is formatted by the `tinymist.statusBarFormat` setting, with the placeholders `{compileStatusIcon}`, `{wordCount}`, `{charCount}`, `{pageCount}`, and `{fileName}`. The indicator will be hidden if the format string is empty.

  Hovering on the indicator shows the main file path and the word, character, space, and CJK character counts of the document. Clicking the indicator runs the `tinymist.showLog` command to show the logs of the language server.

  = Problem Indicator

  The general _Problem Indicator_ in the status bar and the `Problems` panel show the diagnostics from the language server:
  - Compile diagnostics are reported with the source `typst`.
  - Lint diagnostics are reported with the source `tinymist-lint`, when the lint checks are enabled.
  - Diagnostics are published when the document is opened, changed, or saved, so they appear in the `Problems` panel without extra configuration.

  = Typst-Related Commands

  You can press `Ctrl+Shift+P` to open the command palette and type `Typst` or `tinymist` to see all _Typst-Related Commands_. The command surface includes:
  - Export commands: export the current document to PDF, SVG, PNG, plain text, HTML, Markdown, TeX, query results, or a bundle. The `tinymist.exportCurrentFile` command shows a quick pick of the available formats.
  - Preview commands: start and kill the preview of the document in a webview panel, a browser, or in slide mode.
  - Pin commands: `tinymist.pinMainToCurrent`, `tinymist.unpinMain`, and `tinymist.focusMain` for multi-file projects.
  - Template commands: `tinymist.initTemplate`, `tinymist.initTemplateInPlace`, and `tinymist.showTemplateGallery`.
  - Profiling commands: `tinymist.profileCurrentFile` and `tinymist.profileServer`.
  - Server commands: `tinymist.showLog`, `tinymist.restartServer`, and `tinymist.clearCache`.
  - Task system: tasks of type `typst` in the `tasks.json` file are provided to VS Code's task system.
  - Debugging: `tinymist.debugCurrentFile` starts a debugging session of the current document through the Debug Adapter Protocol.

  The editor also shows code lenses on the first line of a document for profiling, previewing, and exporting the document.

  = Drag and Drop

  You can perform _Drag and Drop_ of resource files into the editor to insert them into the document. The dropping and pasting of resources are controlled by the `tinymist.dragAndDrop` and `tinymist.copyAndPaste` settings, and the inserted path is computed by the `tinymist.onPaste` script.

  = Context Menu

  You can `Right Click` in the editor to see the _Context Menu_:
  - `tinymist.exportCurrentPdfFromContext`: export the current document to PDF.
  - `tinymist.copyAnsiHighlight`: copy the syntax highlighted document in ANSI escape sequences.
  - `tinymist.viewAst`: view the abstract syntax tree of the current document.

  = Tinymist LSP

  You can interact with the _Tinymist LSP_ under the state model _Entry File_, _Focus State_, _Pin State_, and _Preview State_.

  == Entry File

  The _Entry File_ is the main file that the language server compiles and analyzes for the current document. It is resolved by the `tinymist.projectResolution` setting:
  - `singleFile` (default): each document is compiled like a Markdown document, where each single file is an individual document and no project resolution is needed.
  - `lockDatabase`: for each workspace, tinymist tracks your preview and compilation history in a lock file, and automatically selects the main file according to the lock file.

  An explicit entry file can also be provided by the `tinymist.typstExtraArgs` setting.

  == Focus State

  The currently focused document is the implicit entry file. Opening a document focuses it implicitly, and you can focus a document explicitly by the `tinymist.focusMain` command. A browsing preview always follows the focused document.

  == Pin State

  You can pin a specific main file by the `tinymist.pinMainToCurrent` command, and unpin it by the `tinymist.unpinMain` command. The pin state resolves by the following precedence:
  - The user's pin always wins.
  - Otherwise, a non-browsing preview pins the file it was launched from.
  - Otherwise, the default entry file (the focused document) is used.

  The pin state is kept in the language server process, so it is not remembered across editor sessions.

  == Preview State

  There are following forms of previews:
  - _Document preview_: shows the compiled document in a webview panel or the browser, and can scroll with the editor.
  - _Slide preview_: shows the document in the slide mode, for documents that are made of slides.
  - _Background preview_: runs a headless preview service without any window. It serves as the primary preview task, so starting a default preview will reuse it instead of creating a new one.

  The _Content View_ and _Outline View_ show the content of the current document when a preview is running.
]

#let neovim-spec = md-spec[
  This document describes the software specification of the Neovim integration for Tinymist.

  = Overview

  There are following integration surfaces for Tinymist in Neovim:
  - You can install the `tinymist` binary and start it as the language server of `typst` files; see _Installation_.
  - You can configure the language server through `nvim-lspconfig`, the built-in `vim.lsp` framework, or `coc.nvim`; see _Configuration_.
  - The repository ships a canonical Neovim plugin and an integration test suite for the Neovim support; see _Canonical Plugin_ and _Testing Suite_.
  - You can use language features like completion, hover, renaming, code actions, folding ranges, and inlay hints through the `vim.lsp` APIs; see _Editor Features_.
  - You can format documents by `vim.lsp.buf.format()` or the `gq` command; see _Formatting_.
  - You can export the document to PDF files on saving or typing; see _Document Export_.
  - You can pin the main file in multi-file projects; see _Multi-File Projects_.
  - You can run live previews of the document; see _Live Preview_.
  - You can debug documents with `nvim-dap`; see _Debugging_.
  - You can troubleshoot the integration by `:checkhealth` and log files; see _Troubleshooting_.

  = Installation

  You can install the `tinymist` binary in following ways:
  - Download a prebuilt binary from the GitHub `Releases` page and put it into your `PATH`.
  - Install it by `:MasonInstall tinymist` if you use `mason.nvim`. Note that the `ensure_installed` option in the `setup` call is a LazyVim-specific convention: plain `lazy.nvim` or hand-written configurations must use `:MasonInstall tinymist` instead.
  - Build and install it by `cargo install --git https://github.com/Myriad-Dreamin/tinymist tinymist-cli`, which installs a binary named `tinymist`.

  Neovim 0.12 and later detect `.typ` files as the `typst` filetype natively, so no filetype plugin is needed for recent versions.

  = Configuration

  There are following channels to configure and start the language server:
  - `nvim-lspconfig`: call `require("lspconfig")["tinymist"].setup { settings = { ... } }` with the settings in the _Typst-Specific LSP Configurations_ section.
  - The built-in `vim.lsp` framework: define `vim.lsp.config["tinymist"] = { cmd = { "tinymist" }, filetypes = { "typst" }, settings = { ... } }` and then call `vim.lsp.enable("tinymist")`. The `vim.lsp.enable` call is required: defining the configuration alone never starts the server.
  - `coc.nvim`: add a `tinymist` entry to the `languageserver` section of `coc-settings.json`.

  The settings in the _Typst-Specific LSP Configurations_ section are sent to the server through the `settings` field. Some settings are only read at server startup, so changing them requires restarting the editor or the server.

  = Canonical Plugin

  The repository ships a canonical Neovim plugin in the `editors/neovim` directory of the tinymist repository:
  - `require("tinymist").setup { lsp = { ... } }` configures and starts the language server. The `lsp` field accepts the same options as the `nvim-lspconfig` setup, and its `init_options` field is another channel to deliver configuration at server startup.
  - `require("tinymist").subscribeDevEvent(...)` subscribes to the `tinymist/devEvent` notifications, which report export events of the server. The events are only sent when the `development` option is enabled, and are mainly used by the integration test suite.

  The plugin is the reference implementation of the Neovim support. The samples directory (`samples/`) contains configurations for LazyVim and NixVim.

  = Editor Features

  The language server attaches to `typst` buffers and provides language features through the `vim.lsp` APIs. Neovim does not create default keymaps for language features, so the keymaps are provided by `nvim-lspconfig` or your configuration:
  - Completion: `vim.lsp` provides omnifunc completion without extra configuration.
  - Hover: show the signature and documentation of the symbol under the cursor, for example by `K` in `nvim-lspconfig`.
  - Goto definition, references, and document symbols: for example by `gd`, `gr`, and the symbol commands.
  - Rename: rename a symbol across the workspace, for example by `vim.lsp.buf.rename()`.
  - Code actions: apply quick fixes and refactors, for example by `vim.lsp.buf.code_action()`.
  - Folding: `vim.lsp.foldexpr()` provides folding ranges based on the syntax structure.
  - Inlay hints: `vim.lsp.inlay_hint.enable()` shows parameter name hints.

  Diagnostics are published to `vim.diagnostic` without extra configuration: compile diagnostics are reported with the source `typst`, and lint diagnostics are reported with the source `tinymist-lint` when the lint checks are enabled.

  = Formatting

  You can format documents by `vim.lsp.buf.format()` or the `gq` command when a formatter mode like `typstyle` or `typstfmt` is configured. The formatting capability is registered dynamically after the server starts, so calling `vim.lsp.buf.format()` immediately after attaching can fail with no matching language servers, and `gq` falls back to Neovim's internal text reflow in that case. Wait for the registration to complete before formatting.

  = Document Export

  You can export the current document to a PDF file by the `tinymist.exportPdf` setting:
  - `onType`: export the document as you type.
  - `onSave`: export the document when it is saved.

  The export writes the PDF file next to the source file by default, and the output location can be changed by the `outputPath` setting. The export task only runs when the document compiles successfully: a document with compile errors is skipped without a message.

  Other export formats are available through the `tinymist.exportSvg`, `tinymist.exportPng`, `tinymist.exportText`, `tinymist.exportHtml`, `tinymist.exportMarkdown`, and `tinymist.exportTeX` commands.

  = Multi-File Projects

  You can pin the main file in multi-file projects by the `tinymist.pinMain` and `tinymist.focusMain` commands:
  - Pin the current buffer's file as the main file by `client:exec_cmd({ command = "tinymist.pinMain", arguments = { vim.api.nvim_buf_get_name(0) } })`.
  - Unpin it by passing `vim.v.null` as the argument.
  - The samples directory of the repository provides keymaps like `<leader>tp` and `<leader>tu` for pinning and unpinning.

  The pin state is kept in the language server process, so it is not remembered across editor sessions.

  = Live Preview

  There are following ways to run a live preview:
  - The `typst-preview.nvim` plugin starts a web preview of the document. The plugin downloads and runs its own `tinymist` binary for the preview, so its version can differ from the language server you configured.
  - The `tinymist.startDefaultPreview` command starts a browsing preview of the focused document, which opens a local web page.
  - The background preview runs a headless preview service. It is only started at server startup when `preview.background.enabled` is on in the server's initialization options: sending the setting through the `settings` channel after startup does not start it.

  = Debugging

  You can debug documents with `nvim-dap`:
  - The debug adapter is the `tinymist` binary itself, started as `tinymist dap`.
  - A minimal configuration declares an adapter of type `executable` with the command `tinymist dap`, and a configuration for the `typst` filetype with the `program` field pointing to the document.
  - The debug session supports stopping on entry, evaluating expressions, and an interactive REPL.

  = Testing Suite

  The repository contains an integration test suite for the Neovim support in the `editors/neovim/spec` directory. The suite covers document export, document summary, DAP debugging, lock file projects, and on-save export. It is designed to run in a container by the `bootstrap.sh` script; running it on a host requires patching the container-specific paths in the test cases.

  = Troubleshooting

  You can troubleshoot the integration in following ways:
  - `:checkhealth vim.lsp` shows the status, version, command, settings, and attached buffers of the language server client. The `:LspInfo` command no longer exists in recent `nvim-lspconfig` versions.
  - `:messages` shows the messages of Neovim and the client.
  - The client writes the language server's log to `stdpath('state') .. '/lsp.log'`, which records the full stderr of the language server.
]

#context {
  let is-vscode = is-vscode.get()
  if is-vscode { vscode-spec } else { neovim-spec }
}
