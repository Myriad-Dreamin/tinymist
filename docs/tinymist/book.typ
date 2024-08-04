
#import "@preview/shiroa:0.1.0": *

#show: book

#book-meta(
  title: "Tinymist Docs",
  description: "The documentation for tinymist service",
  authors: ("Myriad-Dreamin",),
  repository-edit: "https://github.com/Myriad-Dreamin/tinymist/edit/main/{path}",
  language: "en",
  summary: [
    #prefix-chapter("introduction.typ")[Introduction]
    = Features
    - #chapter("feature/cli.typ")[Command line interface]
    - #chapter("feature/language.typ")[Language and Editor Features]
    - #chapter("feature/export.typ")[Exporting Documents]
    - #chapter("feature/preview.typ")[Document Preview]
    = Guide
    - #chapter("guide/completion.typ")[Completion]
    = Editor Integration
    #prefix-chapter("configurations.typ")[Common Configurations]
    - #chapter("frontend/main.typ")[Editor Frontends]
      - #chapter("frontend/vscode.typ")[VS Cod(e,ium)]
      - #chapter("frontend/neovim.typ")[NeoVim]
      - #chapter("frontend/helix.typ")[Helix]
      - #chapter("frontend/zed.typ")[Zed]
    = Service Overview
    #prefix-chapter("overview.typ")[Overview of Service]
    - #chapter("principles.typ")[Principles]
    - #chapter("commands.typ")[Commands System]
    - #chapter("inputs.typ")[LSP Inputs]
    - #chapter("type-system.typ")[Type System]
    = Service Development
    - #chapter("crate-docs.typ")[Crate Docs]
    - #chapter("module/lsp.typ")[LSP and CLI]
    - #chapter("module/query.typ")[Language Queries]
    - #chapter("module/preview.typ")[Document Preview]
  ],
)

#build-meta(dest-dir: "../../dist/tinymist")

#get-book-meta()

// re-export page template
#import "/typ/templates/page.typ": project
#let book-page = project
