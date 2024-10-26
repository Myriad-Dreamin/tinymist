<!-- This file is generated by scripts/link-docs.mjs from docs/tinymist/introduction.typ. Do not edit manually. -->
# Tinymist

Tinymist [ˈtaɪni mɪst] is an integrated language service for [Typst](https://typst.app/) [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see [tinymist-query](https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query).
- a CLI for Typst, see [tinymist](https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist/).
  - which provides a language server for Typst, see [Language Features](https://myriad-dreamin.github.io/tinymist//feature/language.html).
  - which provides a preview server for Typst, see [Preview Feature](https://myriad-dreamin.github.io/tinymist//feature/preview.html).
- a VSCode extension for Typst, see [Tinymist VSCode Extension](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/).

## Features




Language service (LSP) features:

- [Semantic highlighting](https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide)
  - The "semantic highlighting" is supplementary to ["syntax highlighting"](https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide).

- [Code actions](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#provide-code-actions)
  - Also known as "quick fixes" or "refactorings".
- [Formatting (Reformatting)](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#format-source-code-in-an-editor)
  - Provide the user with support for formatting whole documents, using [typstfmt](https://github.com/astrale-sharp/typstfmt) or [typstyle](https://github.com/Enter-tainer/typstyle).
- [Document highlight](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#highlight-all-occurrences-of-a-symbol-in-a-document)
  - Highlight all break points in a loop context.
  - (Todo) Highlight all exit points in a function context.
  - (Todo) Highlight all captures in a closure context.
  - (Todo) Highlight all occurrences of a symbol in a document.
- [Document links](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink)
  - Renders path or link references in the document, such as `image("path.png")` or `bibliography(style: "path.csl")`.
- [Document symbols](https://code.visualstudio.com/docs/getstarted/userinterface#_outline-view)
  - Also known as "document outline" or "table of contents" _in Typst_.
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
- [Rename symbols and embedded paths](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#rename-symbols)
- [Help with function and method signatures](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#help-with-function-and-method-signatures)
- [Workspace Symbols](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-all-symbol-definitions-in-folder)
- [Code Action](https://learn.microsoft.com/en-us/dynamics365/business-central/dev-itpro/developer/devenv-code-actions)
  - Increasing/Decreasing heading levels.
- [experimental/onEnter](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter)
  - <kbd>Enter</kbd> inside triple-slash comments automatically inserts `///`
  - <kbd>Enter</kbd> in the middle or after a trailing space in `//` inserts `//`
  - <kbd>Enter</kbd> inside `//!` doc comments automatically inserts `//!`

Extra features:

- Compiles to PDF on save (configurable to as-you-type, or other options).
- Compiles to SVG, PNG, HTML, Markdown, Text, and other formats by commands, vscode tasks, or code lenses.
- Provides code lenses for exporting to PDF/SVG/PNG/etc.
- Provides a status bar item to show the current document's compilation status and words count.
- [Editor tools](https://github.com/Myriad-Dreamin/tinymist/tree/main/tools/editor-tools):
  - View a list of templates in template gallery. (`tinymist.showTemplateGallery`)
  - Click a button in template gallery to initialize a new project with a template. (`tinymist.initTemplate` and `tinymist.initTemplateInPlace`)
  - Trace execution in current document (`tinymist.profileCurrentFile`).


## Versioning and Release Cycle

Tinymist's versions follow the [Semantic Versioning](https://semver.org/) scheme, in format of `MAJOR.MINOR.PATCH`. Besides, tinymist follows special rules for the version number:
- If a version is suffixed with `-rcN` (<picture><source media="(prefers-color-scheme: dark)" srcset="data:image/svg+xml;base64,PHN2ZyBjbGFzcz0idHlwc3QtZG9jIiB2aWV3Qm94PSIwIDAgMzAuMTY4MTExMTExMTExMTEzIDE3LjQxMyIgd2lkdGg9IjMwLjE2ODExMTExMTExMTExM3B0IiBoZWlnaHQ9IjE3LjQxM3B0IiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIiB4bWxuczpoNT0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94aHRtbCI+CiAgICA8cGF0aCBjbGFzcz0idHlwc3Qtc2hhcGUiIGZpbGw9IiNmZmZmZmYwMCIgZmlsbC1ydWxlPSJub256ZXJvIiBkPSJNIDAgMCBMIDAgMTcuNDEzIEwgMzAuMTY4MTEyIDE3LjQxMyBMIDMwLjE2ODExMiAwIFogIi8+CiAgICA8Zz4KICAgICAgICA8ZyB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwIDEyLjQ2MykiPgogICAgICAgICAgICA8ZyBjbGFzcz0idHlwc3QtdGV4dCIgdHJhbnNmb3JtPSJzY2FsZSgxLCAtMSkiPgogICAgICAgICAgICAgICAgPHVzZSB4bGluazpocmVmPSIjZ0RDMjQzMjE3MkQxQzc1NDBBRjMwNjBENUY1ODE2OTMwIiB4PSIwIiBmaWxsPSIjYzBjYWY1IiBmaWxsLXJ1bGU9Im5vbnplcm8iLz4KICAgICAgICAgICAgPC9nPgogICAgICAgIDwvZz4KICAgICAgICA8ZyB0cmFuc2Zvcm09InRyYW5zbGF0ZSgxMy4wNTQ1NTU1NTU1NTU1NTYgMTIuNDYzKSI+CiAgICAgICAgICAgIDxnIGNsYXNzPSJ0eXBzdC10ZXh0IiB0cmFuc2Zvcm09InNjYWxlKDEsIC0xKSI+CiAgICAgICAgICAgICAgICA8dXNlIHhsaW5rOmhyZWY9IiNnMzE0NzY3MDkxNjg3NDUzRUI5MDBEOEU3OTRFODA4MDUiIHg9IjAiIGZpbGw9IiNjMGNhZjUiIGZpbGwtcnVsZT0ibm9uemVybyIvPgogICAgICAgICAgICA8L2c+CiAgICAgICAgPC9nPgogICAgICAgIDxnIHRyYW5zZm9ybT0idHJhbnNsYXRlKDI0LjY2ODExMTExMTExMTExMyAxMi40NjMpIj4KICAgICAgICAgICAgPGcgY2xhc3M9InR5cHN0LXRleHQiIHRyYW5zZm9ybT0ic2NhbGUoMSwgLTEpIj4KICAgICAgICAgICAgICAgIDx1c2UgeGxpbms6aHJlZj0iI2c1OTUwREVFNDlCQkQzMjQxMEQ3MDlFRjdCNDgyRURBIiB4PSIwIiBmaWxsPSIjYzBjYWY1IiBmaWxsLXJ1bGU9Im5vbnplcm8iLz4KICAgICAgICAgICAgPC9nPgogICAgICAgIDwvZz4KICAgIDwvZz4KICAgIDxkZWZzIGlkPSJnbHlwaCI+CiAgICAgICAgPHN5bWJvbCBpZD0iZ0RDMjQzMjE3MkQxQzc1NDBBRjMwNjBENUY1ODE2OTMwIiBvdmVyZmxvdz0idmlzaWJsZSI+CiAgICAgICAgICAgIDxwYXRoIGQ9Ik0gOS41MDQgNy41MTMgQyA5LjI5NSA3LjUxMyA4LjYxMyA3LjQ4IDguNDA0IDcuNDggQyA4LjE5NSA3LjQ4IDcuNTAyIDcuNTEzIDcuMjkyOTk5NyA3LjUxMyBDIDcuMTI4IDcuNTEzIDcuMDUxIDcuNDI1IDcuMDUxIDcuMjQ5IEMgNy4wNTEgNy4xNSA3LjEyOCA3LjA5NSA3LjI5Mjk5OTcgNy4wODQgQyA3Ljc1NSA3LjA4NCA3Ljk4NiA2Ljk0MSA3Ljk4NiA2LjY1NDk5OTcgQyA3Ljk4NiA2LjU4ODk5OTcgNy45NzUgNi41MTIgNy45NjM5OTk3IDYuNDM1IEwgNi43NzYgMS43MjcgTCA0LjQxMSA3LjI4MiBDIDQuMzIzIDcuNDkxIDQuMzAxIDcuNTEzIDQuMDI2IDcuNTEzIEwgMi41NjMgNy41MTMgQyAyLjI5OSA3LjUxMyAyLjIgNy40OTEgMi4yIDcuMjQ5IEMgMi4yIDcuMTM5IDIuMzIxIDcuMDg0IDIuNTUyIDcuMDg0IEMgMy4wMTQgNy4wODQgMy4yNDUgNy4wNzMgMy4yNTYgNy4wNCBMIDEuNzkzIDEuMjEgQyAxLjY5NCAwLjc4MSAxLjQ0MSAwLjUzOSAxLjA0NSAwLjQ2MiBDIDAuNzU5IDAuNDI5IDAuNDI5IDAuNDczIDAuNDI5IDAuMTY0OTk5OTkgQyAwLjQyOSAwLjA1NSAwLjQ5NSAwIDAuNjE2IDAgQyAwLjgxNCAwIDEuNDk2IDAuMDMzIDEuNzA1IDAuMDMzIEMgMS45MTQgMC4wMzMgMi42MTggMCAyLjgyNyAwIEMgMi45OTIgMCAzLjA2OSAwLjA4OCAzLjA2OSAwLjI2NCBDIDMuMDY5IDAuMzYzIDIuOTgxIDAuNDE3OTk5OTggMi44MDUgMC40MjkgQyAyLjM1NCAwLjQ0IDIuMTM0IDAuNTgzIDIuMTM0IDAuODU4IEMgMi4xMzQgMC45MTMgMi4xNDUgMC45OSAyLjE2NyAxLjEgTCAzLjU4NiA2LjcyMSBMIDYuMzM2IDAuMjMxIEMgNi40MDIgMC4wNzcgNi40OSAwIDYuNTg4OTk5NyAwIEMgNi42ODc5OTk3IDAgNi43NTQgMC4wODggNi43OTggMC4yNjQgTCA4LjMyNyA2LjMxNCBDIDguNDcgNi44NzUgOC43NzggNy4wNzMgOS40NiA3LjA4NCBDIDkuNjE0IDcuMDk1IDkuNjkxIDcuMTgzIDkuNjkxIDcuMzU4OTk5NyBDIDkuNjU4IDcuNDU4IDkuNjQ3IDcuNTEzIDkuNTA0IDcuNTEzIFogIi8+CiAgICAgICAgPC9zeW1ib2w+CiAgICAgICAgPHN5bWJvbCBpZD0iZzMxNDc2NzA5MTY4NzQ1M0VCOTAwRDhFNzk0RTgwODA1IiBvdmVyZmxvdz0idmlzaWJsZSI+CiAgICAgICAgICAgIDxwYXRoIGQ9Ik0gNy41NDYgMi40OTcgQyA3LjY1NiAyLjU1MiA3LjcxMSAyLjYyOSA3LjcxMSAyLjc1IEMgNy43MTEgMi44NzEgNy42NTYgMi45NDggNy41NDYgMy4wMDMgTCAxLjIzMiA1Ljk5NSBDIDEuMTk5IDYuMDA2IDEuMTU1IDYuMDE3IDEuMTExIDYuMDE3IEMgMC45MzUgNi4wMTcgMC44NDcgNS45MjkgMC44NDcgNS43NDIgQyAwLjg0NyA1LjY0MyAwLjkwMiA1LjU2NiAxLjAwMSA1LjUyMiBMIDYuODc1IDIuNzUgTCAxLjAwMSAtMC4wMjIgQyAwLjkwMiAtMC4wNjYgMC44NDcgLTAuMTQzIDAuODQ3IC0wLjI0MiBDIDAuODQ3IC0wLjQyOSAwLjkzNSAtMC41MTcgMS4xMTEgLTAuNTE3IEMgMS4xNTUgLTAuNTE3IDEuMTk5IC0wLjUwNiAxLjIzMiAtMC40OTUgWiAiLz4KICAgICAgICA8L3N5bWJvbD4KICAgICAgICA8c3ltYm9sIGlkPSJnNTk1MERFRTQ5QkJEMzI0MTBENzA5RUY3QjQ4MkVEQSIgb3ZlcmZsb3c9InZpc2libGUiPgogICAgICAgICAgICA8cGF0aCBkPSJNIDIuNzM5IC0wLjI0MiBDIDQuMjkgLTAuMjQyIDUuMDYgMS4wMTIgNS4wNiAzLjUyIEMgNS4wNiA1LjIwMyA0LjcwOCA2LjMyNSA0LjAxNSA2Ljg3NSBDIDMuNjI5OTk5OSA3LjE3MiAzLjIwMSA3LjMyNTk5OTcgMi43NSA3LjMyNTk5OTcgQyAxLjE5OSA3LjMyNTk5OTcgMC40MjkgNi4wNjEgMC40MjkgMy41MiBDIDAuNDI5IDEuNDk2IDAuOTY4IC0wLjI0MiAyLjczOSAtMC4yNDIgWiBNIDMuOTcxIDUuNzY0IEMgNC4wNDggNS4zNzkgNC4wODEgNC42NzUgNC4wODEgMy42NTIgQyA0LjA4MSAyLjYzOTk5OTkgNC4wMzcgMS44OTIgMy45NiAxLjQwOCBDIDMuODE3IDAuNTI4IDMuNDEgMC4wODggMi43MzkgMC4wODggQyAyLjQ4NiAwLjA4OCAyLjIzMyAwLjE4NyAyLjAwMiAwLjM3NCBDIDEuNzA1IDAuNjI3IDEuNTI5IDEuMTQ0IDEuNDUyIDEuOTM2IEMgMS40MTkgMi4yMTEgMS40MDggMi43ODMgMS40MDggMy42NTIgQyAxLjQwOCA0LjYwOSAxLjQ0MSA1LjI3OTk5OTcgMS40OTYgNS42NDMgQyAxLjU5NSA2LjI0OCAxLjc5MyA2LjYzMyAyLjEwMSA2Ljc5OCBDIDIuMzQzIDYuOTMgMi41NTIgNi45OTYgMi43MzkgNi45OTYgQyAzLjQ1NCA2Ljk5NiAzLjg1IDYuNDEzIDMuOTcxIDUuNzY0IFogIi8+CiAgICAgICAgPC9zeW1ib2w+CiAgICA8L2RlZnM+Cjwvc3ZnPgo="><img style="vertical-align: -0.35em" alt="typst-block" src="data:image/svg+xml;base64,PHN2ZyBjbGFzcz0idHlwc3QtZG9jIiB2aWV3Qm94PSIwIDAgMzAuMTY4MTExMTExMTExMTEzIDE3LjQxMyIgd2lkdGg9IjMwLjE2ODExMTExMTExMTExM3B0IiBoZWlnaHQ9IjE3LjQxM3B0IiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIiB4bWxuczpoNT0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94aHRtbCI+CiAgICA8cGF0aCBjbGFzcz0idHlwc3Qtc2hhcGUiIGZpbGw9IiNmZmZmZmYwMCIgZmlsbC1ydWxlPSJub256ZXJvIiBkPSJNIDAgMCBMIDAgMTcuNDEzIEwgMzAuMTY4MTEyIDE3LjQxMyBMIDMwLjE2ODExMiAwIFogIi8+CiAgICA8Zz4KICAgICAgICA8ZyB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwIDEyLjQ2MykiPgogICAgICAgICAgICA8ZyBjbGFzcz0idHlwc3QtdGV4dCIgdHJhbnNmb3JtPSJzY2FsZSgxLCAtMSkiPgogICAgICAgICAgICAgICAgPHVzZSB4bGluazpocmVmPSIjZ0RDMjQzMjE3MkQxQzc1NDBBRjMwNjBENUY1ODE2OTMwIiB4PSIwIiBmaWxsPSIjYzBjYWY1IiBmaWxsLXJ1bGU9Im5vbnplcm8iLz4KICAgICAgICAgICAgPC9nPgogICAgICAgIDwvZz4KICAgICAgICA8ZyB0cmFuc2Zvcm09InRyYW5zbGF0ZSgxMy4wNTQ1NTU1NTU1NTU1NTYgMTIuNDYzKSI+CiAgICAgICAgICAgIDxnIGNsYXNzPSJ0eXBzdC10ZXh0IiB0cmFuc2Zvcm09InNjYWxlKDEsIC0xKSI+CiAgICAgICAgICAgICAgICA8dXNlIHhsaW5rOmhyZWY9IiNnMzE0NzY3MDkxNjg3NDUzRUI5MDBEOEU3OTRFODA4MDUiIHg9IjAiIGZpbGw9IiNjMGNhZjUiIGZpbGwtcnVsZT0ibm9uemVybyIvPgogICAgICAgICAgICA8L2c+CiAgICAgICAgPC9nPgogICAgICAgIDxnIHRyYW5zZm9ybT0idHJhbnNsYXRlKDI0LjY2ODExMTExMTExMTExMyAxMi40NjMpIj4KICAgICAgICAgICAgPGcgY2xhc3M9InR5cHN0LXRleHQiIHRyYW5zZm9ybT0ic2NhbGUoMSwgLTEpIj4KICAgICAgICAgICAgICAgIDx1c2UgeGxpbms6aHJlZj0iI2c1OTUwREVFNDlCQkQzMjQxMEQ3MDlFRjdCNDgyRURBIiB4PSIwIiBmaWxsPSIjYzBjYWY1IiBmaWxsLXJ1bGU9Im5vbnplcm8iLz4KICAgICAgICAgICAgPC9nPgogICAgICAgIDwvZz4KICAgIDwvZz4KICAgIDxkZWZzIGlkPSJnbHlwaCI+CiAgICAgICAgPHN5bWJvbCBpZD0iZ0RDMjQzMjE3MkQxQzc1NDBBRjMwNjBENUY1ODE2OTMwIiBvdmVyZmxvdz0idmlzaWJsZSI+CiAgICAgICAgICAgIDxwYXRoIGQ9Ik0gOS41MDQgNy41MTMgQyA5LjI5NSA3LjUxMyA4LjYxMyA3LjQ4IDguNDA0IDcuNDggQyA4LjE5NSA3LjQ4IDcuNTAyIDcuNTEzIDcuMjkyOTk5NyA3LjUxMyBDIDcuMTI4IDcuNTEzIDcuMDUxIDcuNDI1IDcuMDUxIDcuMjQ5IEMgNy4wNTEgNy4xNSA3LjEyOCA3LjA5NSA3LjI5Mjk5OTcgNy4wODQgQyA3Ljc1NSA3LjA4NCA3Ljk4NiA2Ljk0MSA3Ljk4NiA2LjY1NDk5OTcgQyA3Ljk4NiA2LjU4ODk5OTcgNy45NzUgNi41MTIgNy45NjM5OTk3IDYuNDM1IEwgNi43NzYgMS43MjcgTCA0LjQxMSA3LjI4MiBDIDQuMzIzIDcuNDkxIDQuMzAxIDcuNTEzIDQuMDI2IDcuNTEzIEwgMi41NjMgNy41MTMgQyAyLjI5OSA3LjUxMyAyLjIgNy40OTEgMi4yIDcuMjQ5IEMgMi4yIDcuMTM5IDIuMzIxIDcuMDg0IDIuNTUyIDcuMDg0IEMgMy4wMTQgNy4wODQgMy4yNDUgNy4wNzMgMy4yNTYgNy4wNCBMIDEuNzkzIDEuMjEgQyAxLjY5NCAwLjc4MSAxLjQ0MSAwLjUzOSAxLjA0NSAwLjQ2MiBDIDAuNzU5IDAuNDI5IDAuNDI5IDAuNDczIDAuNDI5IDAuMTY0OTk5OTkgQyAwLjQyOSAwLjA1NSAwLjQ5NSAwIDAuNjE2IDAgQyAwLjgxNCAwIDEuNDk2IDAuMDMzIDEuNzA1IDAuMDMzIEMgMS45MTQgMC4wMzMgMi42MTggMCAyLjgyNyAwIEMgMi45OTIgMCAzLjA2OSAwLjA4OCAzLjA2OSAwLjI2NCBDIDMuMDY5IDAuMzYzIDIuOTgxIDAuNDE3OTk5OTggMi44MDUgMC40MjkgQyAyLjM1NCAwLjQ0IDIuMTM0IDAuNTgzIDIuMTM0IDAuODU4IEMgMi4xMzQgMC45MTMgMi4xNDUgMC45OSAyLjE2NyAxLjEgTCAzLjU4NiA2LjcyMSBMIDYuMzM2IDAuMjMxIEMgNi40MDIgMC4wNzcgNi40OSAwIDYuNTg4OTk5NyAwIEMgNi42ODc5OTk3IDAgNi43NTQgMC4wODggNi43OTggMC4yNjQgTCA4LjMyNyA2LjMxNCBDIDguNDcgNi44NzUgOC43NzggNy4wNzMgOS40NiA3LjA4NCBDIDkuNjE0IDcuMDk1IDkuNjkxIDcuMTgzIDkuNjkxIDcuMzU4OTk5NyBDIDkuNjU4IDcuNDU4IDkuNjQ3IDcuNTEzIDkuNTA0IDcuNTEzIFogIi8+CiAgICAgICAgPC9zeW1ib2w+CiAgICAgICAgPHN5bWJvbCBpZD0iZzMxNDc2NzA5MTY4NzQ1M0VCOTAwRDhFNzk0RTgwODA1IiBvdmVyZmxvdz0idmlzaWJsZSI+CiAgICAgICAgICAgIDxwYXRoIGQ9Ik0gNy41NDYgMi40OTcgQyA3LjY1NiAyLjU1MiA3LjcxMSAyLjYyOSA3LjcxMSAyLjc1IEMgNy43MTEgMi44NzEgNy42NTYgMi45NDggNy41NDYgMy4wMDMgTCAxLjIzMiA1Ljk5NSBDIDEuMTk5IDYuMDA2IDEuMTU1IDYuMDE3IDEuMTExIDYuMDE3IEMgMC45MzUgNi4wMTcgMC44NDcgNS45MjkgMC44NDcgNS43NDIgQyAwLjg0NyA1LjY0MyAwLjkwMiA1LjU2NiAxLjAwMSA1LjUyMiBMIDYuODc1IDIuNzUgTCAxLjAwMSAtMC4wMjIgQyAwLjkwMiAtMC4wNjYgMC44NDcgLTAuMTQzIDAuODQ3IC0wLjI0MiBDIDAuODQ3IC0wLjQyOSAwLjkzNSAtMC41MTcgMS4xMTEgLTAuNTE3IEMgMS4xNTUgLTAuNTE3IDEuMTk5IC0wLjUwNiAxLjIzMiAtMC40OTUgWiAiLz4KICAgICAgICA8L3N5bWJvbD4KICAgICAgICA8c3ltYm9sIGlkPSJnNTk1MERFRTQ5QkJEMzI0MTBENzA5RUY3QjQ4MkVEQSIgb3ZlcmZsb3c9InZpc2libGUiPgogICAgICAgICAgICA8cGF0aCBkPSJNIDIuNzM5IC0wLjI0MiBDIDQuMjkgLTAuMjQyIDUuMDYgMS4wMTIgNS4wNiAzLjUyIEMgNS4wNiA1LjIwMyA0LjcwOCA2LjMyNSA0LjAxNSA2Ljg3NSBDIDMuNjI5OTk5OSA3LjE3MiAzLjIwMSA3LjMyNTk5OTcgMi43NSA3LjMyNTk5OTcgQyAxLjE5OSA3LjMyNTk5OTcgMC40MjkgNi4wNjEgMC40MjkgMy41MiBDIDAuNDI5IDEuNDk2IDAuOTY4IC0wLjI0MiAyLjczOSAtMC4yNDIgWiBNIDMuOTcxIDUuNzY0IEMgNC4wNDggNS4zNzkgNC4wODEgNC42NzUgNC4wODEgMy42NTIgQyA0LjA4MSAyLjYzOTk5OTkgNC4wMzcgMS44OTIgMy45NiAxLjQwOCBDIDMuODE3IDAuNTI4IDMuNDEgMC4wODggMi43MzkgMC4wODggQyAyLjQ4NiAwLjA4OCAyLjIzMyAwLjE4NyAyLjAwMiAwLjM3NCBDIDEuNzA1IDAuNjI3IDEuNTI5IDEuMTQ0IDEuNDUyIDEuOTM2IEMgMS40MTkgMi4yMTEgMS40MDggMi43ODMgMS40MDggMy42NTIgQyAxLjQwOCA0LjYwOSAxLjQ0MSA1LjI3OTk5OTcgMS40OTYgNS42NDMgQyAxLjU5NSA2LjI0OCAxLjc5MyA2LjYzMyAyLjEwMSA2Ljc5OCBDIDIuMzQzIDYuOTMgMi41NTIgNi45OTYgMi43MzkgNi45OTYgQyAzLjQ1NCA2Ljk5NiAzLjg1IDYuNDEzIDMuOTcxIDUuNzY0IFogIi8+CiAgICAgICAgPC9zeW1ib2w+CiAgICA8L2RlZnM+Cjwvc3ZnPgo="/></picture>), e.g. `0.11.0-rc1` and `0.12.1-rc1`, it means this version is a release candidate. It is used to test publish script and E2E functionalities. These versions will not be published to the marketplace.
- If the `PATCH` number is odd, e.g. `0.11.1` and `0.12.3`, it means this version is a nightly release. The nightly release will use both [tinymist](https://github.com/Myriad-Dreamin/tinymist/tree/main) and [typst](https://github.com/typst/typst/tree/main) at **main branch**. They will be published as prerelease version to the marketplace.
- Otherwise, if the `PATCH` number is even, e.g. `0.11.0` and `0.12.2`, it means this version is a regular release. The regular release will always use the recent stable version of tinymist and typst.

The release cycle is as follows:
- If there is a typst version update, a new major or minor version will be released intermediately. This means tinymist will always align the minor version with typst.
- If there is at least a bug or feature added this week, a new patch version will be released.

## Installation

Follow the instructions to enable tinymist in your favorite editor.
- [VS Cod(e,ium)](https://myriad-dreamin.github.io/tinymist//frontend/vscode.html)
- [NeoVim](https://myriad-dreamin.github.io/tinymist//frontend/neovim.html)
- [Emacs](https://myriad-dreamin.github.io/tinymist//frontend/emacs.html)
- [Sublime Text](https://myriad-dreamin.github.io/tinymist//frontend/sublime-text.html)
- [Helix](https://myriad-dreamin.github.io/tinymist//frontend/helix.html)
- [Zed](https://myriad-dreamin.github.io/tinymist//frontend/zed.html)

## Installing Regular/Nightly Prebuilds from GitHub

Note: if you are not knowing what is a regular/nightly release, please don't follow this section.

Besides published releases specific for each editors, you can also download the latest regular/nightly prebuilts from GitHub and install them manually.

- Regular prebuilts can be found in [GitHub Releases](https://github.com/Myriad-Dreamin/tinymist/releases).
- Nightly prebuilts can be found in [GitHub Actions](https://github.com/Myriad-Dreamin/tinymist/actions). For example, if you are seeking a nightly release for the featured [PR: build: bump version to 0.11.17-rc1](https://github.com/Myriad-Dreamin/tinymist/pull/468), you could click and go to the [action page](https://github.com/Myriad-Dreamin/tinymist/actions/runs/10120639466) run for the related commits and download the artifacts.

To install extension file (the file with `.vsix` extension) manually, please <kbd>Ctrl+Shift+X</kbd> in the editor window and drop the downloaded vsix file into the opened extensions view.

## Documentation

See [Online Documentation](https://myriad-dreamin.github.io/tinymist/).

## Packaging

Stable Channel:

[![Packaging status](https://repology.org/badge/vertical-allrepos/tinymist.svg)](https://repology.org/project/tinymist/versions)

Nightly Channel:

[![Packaging status](https://repology.org/badge/vertical-allrepos/tinymist-nightly.svg)](https://repology.org/project/tinymist-nightly/versions)

## Roadmap

- Spell checking: There is already a branch but no suitable (default) spell checking library is found.
- Periscope renderer: It is disabled since vscode reject to render SVGs containing foreignObjects.
- Inlay hint: It is disabled _by default_ because of performance issues.
- Find references of dictionary fields and named function arguments.
- Go to definition of dictionary fields and named function arguments.
- Improve symbol view's appearance.

## Contributing

Please read the [CONTRIBUTING.md](CONTRIBUTING.md) file for contribution guidelines.

## Acknowledgements

- Partially code is inherited from [typst-lsp](https://github.com/nvarner/typst-lsp)
- The [integrating](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#symbol-view) **offline** handwritten-stroke recognizer is powered by [Detypify](https://detypify.quarticcat.com/).
- The [integrating](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#preview-command) preview service is powered by [typst-preview](https://github.com/Enter-tainer/typst-preview).
- The [integrating](https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#managing-local-packages) local package management functions are adopted from [vscode-typst-sync](https://github.com/OrangeX4/vscode-typst-sync).
