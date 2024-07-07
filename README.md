# Tinymist

Tinymist [ˈtaɪni mɪst] is an integrated language service for [Typst](https://typst.app/) [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see [tinymist-query](./crates/tinymist-query/).
- a CLI for Typst, see [tinymist](./crates/tinymist/).
  - which provides a language server for Typst.
- a VSCode extension for Typst, see [Tinymist VSCode Extension](./editors/vscode/).

## Features

Language service (LSP) features:

- [Semantic highlighting](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide)
  - Also known as "syntax highlighting".
- [Diagnostics](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#provide-diagnostics)
  - Also known as "error checking" or "error reporting".
- [Document highlight](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#highlight-all-occurrences-of-a-symbol-in-a-document)
  - Highlight all break points in a loop context.
  - (Todo) Highlight all exit points in a function context.
  - (Todo) Highlight all captures in a closure context.
  - (Todo) Highlight all occurrences of a symbol in a document.
- [Document symbols](https://code.visualstudio.com/docs/getstarted/userinterface#_outline-view)
  - Also known as "document outline" or "table of contents" **in Typst**.
- [Folding ranges](https://burkeholland.gitbook.io/vs-code-can-do-that/exercise-3-navigation-and-refactoring/folding-sections)
  - You can collapse code/content blocks and headings.
- [Goto definitions](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-definitions-of-a-symbol)
  - Right-click on a symbol and select "Go to Definition".
  - Or ctrl+click on a symbol.
- [References](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#find-all-references-to-a-symbol)
  - Right-click on a symbol and select "Go to References" or "Find References".
  - Or ctrl+click on a symbol.
- [Hover tips](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-hovers)
  - Also known as "hovering tooltip".
- [Inlay hints](https://www.jetbrains.com/help/idea/inlay-hints.html)
  - Inlay hints are special markers that appear in the editor and provide you with additional information about your code, like the names of the parameters that a called method expects.
- [Color Provider](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-color-decorators)
  - View all inlay colorful label for color literals in your document.
  - Change the color literal's value by a color picker or its code presentation.
- [Code Lens](https://code.visualstudio.com/blogs/2017/02/12/code-lens-roundup)
  - Should give contextual buttons along with code. For example, a button for exporting your document to various formats at the start of the document.
- [Rename symbols](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#rename-symbols)
- [Help with function and method signatures](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#help-with-function-and-method-signatures)
- [Workspace Symbols](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-all-symbol-definitions-in-folder)
- [Code Action](https://learn.microsoft.com/en-us/dynamics365/business-central/dev-itpro/developer/devenv-code-actions)
  - Increasing/Decreasing heading levels.
- [experimental/onEnter](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter)
  - <kbd>Enter</kbd> inside triple-slash comments automatically inserts `///`
  - <kbd>Enter</kbd> in the middle or after a trailing space in `//` inserts `//`
  - <kbd>Enter</kbd> inside `//!` doc comments automatically inserts `//!`

Extra features:

- Compiles to PDF on save (configurable to as-you-type, or other options)
- Provides code lenses for exporting to various formats (PDF, SVG, PNG, etc.)
- Provides a status bar item to show the current document's compilation status and words count.
- [Editor tools](https://github.com/Myriad-Dreamin/tinymist/tree/main/tools/editor-tools):
  - View a list of templates in template gallery. (`tinymist.showTemplateGallery`)
  - Click a button in template gallery to initialize a new project with a template. (`tinymist.initTemplate` and `tinymist.initTemplateInPlace`)
  - Trace execution in current document. (`tinymist.profileCurrentFile`)


## Installation

Follow the instructions to enable tinymist in your favorite editor.
+ [Helix](./editors/helix/README.md)
+ [Neovim](./editors/neovim/README.md)
+ [VSCode](./editors/vscode/README.md)
+ [Zed](editors/zed/README.md)

## Documentation

See [Online Documentation](https://myriad-dreamin.github.io/tinymist/).

## Packaging

[![Packaging status](https://repology.org/badge/vertical-allrepos/tinymist.svg)](https://repology.org/project/tinymist/versions)

## Roadmap

After development for two months, most of the features are implemented. There are still some features to be implemented, but I would like to leave them in typst v0.12.0. I'll also pick some of them to implement on my weekends. Also please feel free to contribute if you are interested in the following features.

- Documentation and refactoring: It is my current focus.
- Spell checking: There is already a branch but no suitable (default) spell checking library is found.
- Syntax Highlighting for embedded languages in raw blocks: It almost works, but it is disabled because of wrong syntax highlighting on syntax errors in the raw blocks.
- Periscope renderer: It is disabled since vscode reject to render SVGs containing foreignObjects.
- Inlay hint: It is disabled **by default** because of performance issues.
- Find references of labels, dictionary fields, and named function arguments.
- Go to definition of dictionary fields and named function arguments.
- Autocompletion for raw language tags.
- Improve symbol view's appearance.

## Contributing

Please read the [CONTRIBUTING.md](CONTRIBUTING.md) file for contribution guidelines.

## Acknowledgements

- Partially code is inherited from [typst-lsp](https://github.com/nvarner/typst-lsp)
- The [integrating](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#symbol-view) *offline* handwritten-stroke recognizer is powered by [Detypify](https://detypify.quarticcat.com/).
- The [integrating](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#preview) preview service is powered by [typst-preview](https://github.com/Enter-tainer/typst-preview).
