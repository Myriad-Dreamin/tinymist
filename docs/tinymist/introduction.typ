// dist/tinymist/rs
#import "mod.typ": *

#show: book-page.with(title: "Introduction")

Tinymist [ˈtaɪni mɪst] is an integrated language service for #link("https://typst.app/")[Typst] [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query].
- a CLI for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist/")[tinymist].
  - which provides a language server for Typst, see #cross-link("/feature/language.typ")[Langauge Features].
  - which provides a preview server for Typst, see #cross-link("/feature/preview.typ")[Preview Feature].
- a VSCode extension for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/")[Tinymist VSCode Extension].

== Features

Language service (LSP) features:

- #link("https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide")[Semantic highlighting]
  - Also known as "syntax highlighting".
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#provide-diagnostics")[Diagnostics]
  - Also known as "error checking" or "error reporting".
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#highlight-all-occurrences-of-a-symbol-in-a-document")[Document highlight]
  - Highlight all break points in a loop context.
  - (Todo) Highlight all exit points in a function context.
  - (Todo) Highlight all captures in a closure context.
  - (Todo) Highlight all occurrences of a symbol in a document.
- #link("https://code.visualstudio.com/docs/getstarted/userinterface#_outline-view")[Document symbols]
  - Also known as "document outline" or "table of contents" _in Typst_.
- #link("https://burkeholland.gitbook.io/vs-code-can-do-that/exercise-3-navigation-and-refactoring/folding-sections")[Folding ranges]
  - You can collapse code/content blocks and headings.
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-definitions-of-a-symbol")[Goto definitions]
  - Right-click on a symbol and select "Go to Definition".
  - Or ctrl+click on a symbol.
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#find-all-references-to-a-symbol")[References]
  - Right-click on a symbol and select "Go to References" or "Find References".
  - Or ctrl+click on a symbol.
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-hovers")[Hover tips]
  - Also known as "hovering tooltip".
- #link("https://www.jetbrains.com/help/idea/inlay-hints.html")[Inlay hints]
  - Inlay hints are special markers that appear in the editor and provide you with additional information about your code, like the names of the parameters that a called method expects.
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-color-decorators")[Color Provider]
  - View all inlay colorful label for color literals in your document.
  - Change the color literal's value by a color picker or its code presentation.
- #link("https://code.visualstudio.com/blogs/2017/02/12/code-lens-roundup")[Code Lens]
  - Should give contextual buttons along with code. For example, a button for exporting your document to various formats at the start of the document.
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#rename-symbols")[Rename symbols]
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#help-with-function-and-method-signatures")[Help with function and method signatures]
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-all-symbol-definitions-in-folder")[Workspace Symbols]
- #link("https://learn.microsoft.com/en-us/dynamics365/business-central/dev-itpro/developer/devenv-code-actions")[Code Action]
  - Increasing/Decreasing heading levels.
- #link("https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter")[experimental/onEnter]
  - #kbd("Enter") inside triple-slash comments automatically inserts `///`
  - #kbd("Enter") in the middle or after a trailing space in `//` inserts `//`
  - #kbd("Enter") inside `//!` doc comments automatically inserts `//!`

Extra features:

- Compiles to PDF on save (configurable to as-you-type, or other options)
- Provides code lenses for exporting to various formats (PDF, SVG, PNG, etc.)
- Provides a status bar item to show the current document's compilation status and words count.
- #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/tools/editor-tools")[Editor tools]:
  - View a list of templates in template gallery. (`tinymist.showTemplateGallery`)
  - Click a button in template gallery to initialize a new project with a template. (`tinymist.initTemplate` and `tinymist.initTemplateInPlace`)
  - Trace execution in current document. (`tinymist.profileCurrentFile`)

== Installation

Follow the instructions to enable tinymist in your favorite editor.
+ #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/helix/README.md")[Helix]
+ #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim/README.md")[Neovim]
+ #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/README.md")[VSCode]
+ #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/zed/README.md")[Zed]

== Documentation

See #link("https://myriad-dreamin.github.io/tinymist/")[Online Documentation].

== Packaging

#link(
  "https://repology.org/project/tinymist/versions",
  md-alter(
    "Packaging status",
    () => image("https://repology.org/badge/vertical-allrepos/tinymist.svg", alt: "Packaging status"),
  ),
)


== Roadmap

After development for two months, most of the features are implemented. There are still some features to be implemented, but I would like to leave them in typst v0.12.0. I'll also pick some of them to implement on my weekends. Also please feel free to contribute if you are interested in the following features.

- Documentation and refactoring: It is my current focus.
- Spell checking: There is already a branch but no suitable (default) spell checking library is found.
- Periscope renderer: It is disabled since vscode reject to render SVGs containing foreignObjects.
- Inlay hint: It is disabled _by default_ because of performance issues.
- Find references of labels, dictionary fields, and named function arguments.
- Go to definition of dictionary fields and named function arguments.
- Autocompletion for raw language tags.
- Improve symbol view's appearance.

== Contributing

Please read the #link("CONTRIBUTING.md")[CONTRIBUTING.md] file for contribution guidelines.

== Acknowledgements

- Partially code is inherited from #link("https://github.com/nvarner/typst-lsp")[typst-lsp]
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#symbol-view")[integrating] *offline* handwritten-stroke recognizer is powered by #link("https://detypify.quarticcat.com/")[Detypify].
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#preview")[integrating] preview service is powered by #link("https://github.com/Enter-tainer/typst-preview")[typst-preview].
