
#import "../mod.typ": *

Language service (LSP) features:

- #link("https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide")[Semantic highlighting]
  - The "semantic highlighting" is supplementary to #link("https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide")["syntax highlighting"].
// formatting
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#provide-code-actions")[Code actions]
  - Also known as "quick fixes" or "refactorings".
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#format-source-code-in-an-editor")[Formatting (Reformatting)]
  - Provide the user with support for formatting whole documents, using #link("https://github.com/astrale-sharp/typstfmt")[typstfmt] or #link("https://github.com/Enter-tainer/typstyle")[typstyle].
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#highlight-all-occurrences-of-a-symbol-in-a-document")[Document highlight]
  - Highlight all break points in a loop context.
  - (Todo) Highlight all exit points in a function context.
  - (Todo) Highlight all captures in a closure context.
  - (Todo) Highlight all occurrences of a symbol in a document.
- #link("https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentLink")[Document links]
  - Renders path or link references in the document, such as `image("path.png")` or `bibliography(style: "path.csl")`.
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
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#rename-symbols")[Rename symbols and embedded paths]
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#help-with-function-and-method-signatures")[Help with function and method signatures]
- #link("https://code.visualstudio.com/api/language-extensions/programmatic-language-features#show-all-symbol-definitions-in-folder")[Workspace Symbols]
- #link("https://learn.microsoft.com/en-us/dynamics365/business-central/dev-itpro/developer/devenv-code-actions")[Code Action]
  - Increasing/Decreasing heading levels.
- #link("https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter")[experimental/onEnter]
  - #kbd("Enter") inside triple-slash comments automatically inserts `///`
  - #kbd("Enter") in the middle or after a trailing space in `//` inserts `//`
  - #kbd("Enter") inside `//!` doc comments automatically inserts `//!`

Extra features:

- Compiles to PDF on save (configurable to as-you-type, or other options).
- Compiles to SVG, PNG, HTML, Markdown, Text, and other formats by commands, vscode tasks, or code lenses.
- Provides code lenses for exporting to PDF/SVG/PNG/etc.
- Provides a status bar item to show the current document's compilation status and words count.
- #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/tools/editor-tools")[Editor tools]:
  - View a list of templates in template gallery. (`tinymist.showTemplateGallery`)
  - Click a button in template gallery to initialize a new project with a template. (`tinymist.initTemplate` and `tinymist.initTemplateInPlace`)
  - Trace execution in current document (`tinymist.profileCurrentFile`).
