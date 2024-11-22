
#import "typ/templates/maintainer.typ": *
#show: main

#let editor-integration = [Editor Integration]
#let language-service = [Language Service]
#let document-previewing = [Document Previewing]
#let vs-code-client-side-support = [VS Code Client-Side Support]
#let nightly-releases = [Nightly Releases]

== Maintainers

#maintainers[
  - Myriad-Dreamin
    - #github("Myriad-Dreamin")
    - #email("camiyoru@gmail.com")
    - #maintains[
        - #editor-integration
        - #language-service
        - #document-previewing
        - #vs-code-client-side-support
        - #nightly-releases
      ]
  - Enter-tainer
    - #github("Enter-tainer")
    - #email("mgt@oi-wiki.org")
    - #maintains[
        - #editor-integration
        - #language-service
        - #document-previewing
        - #vs-code-client-side-support
      ]
  - ParaN3xus
    - #github("ParaN3xus")
    - #email("paran3xus007@gmail.com")
    - #maintains[
        - #nightly-releases
      ]
  - Max397
    - #github("max397574")
    - #maintains[
        - #editor-integration
      ]
  - Ericoolen
    - #github("Eric-Song-Nop")
    - #email("EricYFSong@gmail.com")
    - #maintains[
        - #language-service
      ]
  - Caleb Maclennan
    - #github("alerque")
    - #email("caleb@alerque.com")
    - #maintains[
        - #editor-integration
      ]
]

== Features

#features[
  - #editor-integration
    - #scope("crates/tinymist/", "editors/")
    - #description[
        Integrate tinymist server with popular editors like VS Code, Neovim, etc.
      ]
  - #language-service
    - #scope("crates/tinymist/", "crates/tinymist-query/")
    - #description[
        Perform code analysis and provide language support for Typst.
      ]
  - #document-previewing
    - #scope(
        "crates/tinymist/",
        "crates/typst-preview/",
        "contrib/typst-preview/",
        "tools/typst-dom/",
        "tools/typst-preview-frontend/",
      )
    - #description[
        Provide instant preview of the document being edited.
      ]
  - #vs-code-client-side-support
    - #scope("crates/tinymist/", "editors/vscode/", "tools/editor-tools/")
    - #description[
        Enrich the VS Code features with the client-side extension.
      ]
  - #nightly-releases
    - #scope("crates/tinymist/", "typst-shim/")
    - #description[
        Build and Publish nightly releases of tinymist. The nightly releases are built upon the main branches of both tinymist and typst.
      ]
]
