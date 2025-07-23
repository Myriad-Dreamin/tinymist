
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
  - You can interacts with the _Tinymist LSP_ under the state model _Entry File_, _Focus State_, _Pin State_, and _Preview State_.
  - You can configure the extension and the language server by the _Typst-Specific VS Code Configurations_.

  = Tinymist Views

  There are following views in the _Tinymist Views_:
  - _Symbol View_
  - _Tool View_
  - _Package View_
  - _Content View_
  - _Label View_
  - _Outline View_

  == Symbol View

  There are following actions that you can perform in the _Symbol View_:
  - Search Symbol by Names: You can search for a symbol by its name. The search is case-insensitive and supports partial matching.
  - Search Symbol by Handwritten Strokes: You can draw a symbol and search for it. The symbol will be highlighted.
  - Click to paste: You can click on a symbol to paste it to the editor.

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

  You can find all syntatical labels in the workspaces in the _Label View_. The labels are grouped by prefix.

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
]

#context {
  let is-vscode = is-vscode.get()
  if is-vscode { vscode-spec } else [ Todo ]
}

