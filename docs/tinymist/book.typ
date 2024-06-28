
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
    = LSP
    #prefix-chapter("overview.typ")[Overview of Service]
    - #chapter("principles.typ")[Principles]
    - #chapter("commands.typ")[Commands System]
    - #chapter("inputs.typ")[LSP Inputs]
    - #chapter("type-system.typ")[Type System]
    - #chapter("language-features.typ")[Language Features]
    = Editor Integration
    #prefix-chapter("configurations.typ")[Common Configurations]
    - #chapter("frontend/main.typ")[Editor Frontends]
      - #chapter("frontend/vscode.typ")[VS Cod(e,ium)]
      - #chapter("frontend/neovim.typ")[NeoVim]
      - #chapter("frontend/helix.typ")[Helix]
      - #chapter("frontend/zed.typ")[Zed]
  ],
)

#build-meta(dest-dir: "../../dist/tinymist")

#get-book-meta()

// re-export page template
#import "/typ/templates/page.typ": project
#let book-page = project
