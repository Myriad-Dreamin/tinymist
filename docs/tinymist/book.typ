
#import "@preview/shiroa:0.2.0": *

#show: book

#book-meta(
  title: "Tinymist Docs",
  description: "The documentation for tinymist service",
  authors: ("Myriad-Dreamin",),
  repository-edit: "https://github.com/Myriad-Dreamin/tinymist/edit/main/{path}",
  language: "en",
  summary: [
    #prefix-chapter("introduction.typ")[Introduction]
    = Editor Integration
    #prefix-chapter("configurations.typ")[Common Configurations]
    - #chapter("frontend/main.typ")[Editor Frontends]
      - #chapter("frontend/vscode.typ")[VS Cod(e,ium)]
      - #chapter("frontend/neovim.typ")[NeoVim]
      - #chapter("frontend/emacs.typ")[Emacs]
      - #chapter("frontend/sublime-text.typ")[Sublime Text]
      - #chapter("frontend/helix.typ")[Helix]
      - #chapter("frontend/zed.typ")[Zed]
    = Features
    - #chapter("feature/cli.typ")[Command line interface]
    - #chapter("feature/docs.typ")[Code Documentation]
    - #chapter("guide/completion.typ")[Code Completion]
    - #chapter("feature/export.typ")[Exporting Documents]
    - #chapter("feature/preview.typ")[Document Preview]
    - #chapter("feature/language.typ")[Other Features]
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
