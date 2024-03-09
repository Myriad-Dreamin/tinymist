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
- [Document symbols](https://code.visualstudio.com/docs/getstarted/userinterface#_outline-view)
  - Also known as "document outline" or "table of contents" **in Typst**.
- [Folding ranges](https://burkeholland.gitbook.io/vs-code-can-do-that/exercise-3-navigation-and-refactoring/folding-sections)
  - You can collapse code/content blocks and headings.
- [Goto definitions](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-definitions-of-a-symbol)
  - Right-click on a symbol and select "Go to Definition".
  - Or ctrl+click on a symbol.
- [Goto declarations](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#find-all-references-to-a-symbol)
  - Also known as "find all references".
- [Hover tips](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-hovers)
  - Also known as "hovering tooltip".
- [Inlay hints](https://www.jetbrains.com/help/idea/inlay-hints.html)
  - Inlay hints are special markers that appear in the editor and provide you with additional information about your code, like the names of the parameters that a called method expects.
- [Rename symbols](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#rename-symbols)
- [Help with function and method signatures](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#help-with-function-and-method-signatures)
- [Workspace Symbols](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-all-symbol-definitions-in-folder)

Extra features:

- Compiles to PDF on save (configurable to as-you-type, or can be disabled)

## Installation

Follow the instructions to enable tinymist in your favorite editor.
+ [VSCode](./editors/vscode/README.md)

## Acknowledgements

- It is developed based on [typst-lsp](https://github.com/nvarner/typst-lsp)
