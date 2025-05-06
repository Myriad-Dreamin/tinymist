# IntelliJ Plugin API Documentation Notes for Tinymist

This file contains links to relevant documentation and summaries for the IntelliJ Platform APIs used in the Tinymist plugin.

## Language Server Protocol (LSP) Integration

*   **Official Documentation:** [Language Server Protocol (LSP)](https://plugins.jetbrains.com/docs/intellij/language-server-protocol.html)
    *   Explains the overall approach, supported IDEs (requires Ultimate/paid versions), setup (`build.gradle.kts`, `plugin.xml` dependencies), and supported LSP features per IDE version.
*   **`LspServerSupportProvider`** (`org.tinymist.intellij.lsp.TinymistLspServerSupportProvider`)
    *   **Purpose:** Entry point to connect files of a specific type (`TypstFileType`) to an LSP server. Registered via the `platform.lsp.serverSupportProvider` extension point in `plugin.xml`.
    *   **Key Method:** `fileOpened(project, file, serverStarter)`: Called when a file is opened. Inside this method, check if the `file` is relevant (e.g., by checking `file.fileType`). If it is, call `serverStarter.ensureServerStarted(descriptor)` with an appropriate `LspServerDescriptor`.
    *   **Docs:** Mentioned within the main LSP documentation page.
*   **`LspServerDescriptor`** (`org.tinymist.intellij.lsp.TinymistLspServerDescriptor`)
    *   **Purpose:** Describes how to start, connect to, and interact with a specific Language Server instance. It's typically project-scoped (e.g., extending `ProjectWideLspServerDescriptor`).
    *   **Key Methods/Properties:**
        *   `isSupportedFile(file)`: Checks if a given `VirtualFile` should be handled by this LSP server.
        *   `createCommandLine()`: Returns a `GeneralCommandLine` object describing how to execute the LSP server (e.g., `GeneralCommandLine("path/to/tinymist", "--lsp")`). This is used for StdIO communication. Socket communication is also possible via other methods.
        *   Customization properties (e.g., `lspGoToDefinitionSupport`, `lspCompletionSupport`, `lspHoverSupport`): Allow enabling/disabling or potentially fine-tuning specific LSP features provided by the platform's integration layer.
        *   `createLsp4jClient()` / `lsp4jServerClass`: Advanced customization points for handling non-standard requests/notifications.
    *   **Docs:** Detailed within the main LSP documentation page and likely in the API source code comments (attach sources in IDE).

## Basic Language Support APIs (Minimal Implementation for LSP)

*   **`ParserDefinition`** (`org.tinymist.intellij.TypstParserDefinition`)
    *   **Purpose:** Defines the bridge between lexing, parsing, and PSI tree creation for a language. Registered via `com.intellij.lang.parserDefinition` extension point. Even with LSP handling most intelligence, a minimal implementation is often required by the platform.
    *   **Key Methods:**
        *   `createLexer(project)`: Returns an instance of the `Lexer` for this language.
        *   `createParser(project)`: Returns an instance of the `PsiParser`. For LSP-heavy plugins, this can be a minimal parser that just creates the root file node.
        *   `getFileNodeType()`: Returns the `IFileElementType` representing the root of a file.
        *   `getWhitespaceTokens()`: Returns a `TokenSet` of token types considered whitespace (usually `TokenSet.WHITE_SPACE`).
        *   `getCommentTokens()`: Returns a `TokenSet` for comment token types.
        *   `getStringLiteralElements()`: Returns a `TokenSet` for string literal tokens.
        *   `createElement(node)`: Creates a `PsiElement` for a given `ASTNode`. Often delegates to a generated factory if using Grammar-Kit, or can be a simple wrapper like `ASTWrapperPsiElement(node)`.
        *   `createFile(viewProvider)`: Creates the `PsiFile` instance for the language (e.g., `TypstFile(viewProvider)`).
    *   **Docs:**
        *   [Implementing Parser and PSI](https://plugins.jetbrains.com/docs/intellij/implementing-parser-and-psi.html)
        *   [Custom Language Support Tutorial: Parser Definition](https://plugins.jetbrains.com/docs/intellij/lexer-and-parser-definition.html)
*   **`Lexer`** (`org.tinymist.intellij.TypstLexerAdapter`)
    *   **Purpose:** Breaks file content into a stream of tokens (`IElementType`). Used by the parser and the syntax highlighter. A minimal implementation is needed even if LSP provides semantic tokens. The lexer *must* cover the entire file content without gaps.
    *   **Key Methods (when implementing `com.intellij.lexer.Lexer` directly):**
        *   `start(buffer, startOffset, endOffset, initialState)`: Initializes the lexer with the text buffer.
        *   `advance()`: Advances to the next token. Updates internal state.
        *   `getTokenType()`: Returns the `IElementType` of the current token. Returns `null` at the end.
        *   `getTokenStart()` / `getTokenEnd()`: Return start/end offsets of the current token.
        *   `getState()`: Returns the lexer state at the *end* of the current token (important for incremental highlighting). Can be `0` for simple/non-incremental lexers.
        *   `getCurrentPosition()`: Returns a `LexerPosition` (offset and state) for checkpointing.
        *   `restore(position)`: Restores the lexer to a previously saved position.
    *   **Alternative:** Use JFlex and `FlexAdapter`. [Implementing Lexer](https://plugins.jetbrains.com/docs/intellij/implementing-lexer.html)
    *   **Docs:**
        *   [Implementing Lexer](https://plugins.jetbrains.com/docs/intellij/implementing-lexer.html)
        *   [Custom Language Support Tutorial: Lexer](https://plugins.jetbrains.com/docs/intellij/lexer-and-parser-definition.html)
*   **`SyntaxHighlighter` / `SyntaxHighlighterFactory`** (`org.tinymist.intellij.TypstSyntaxHighlighter`, `org.tinymist.intellij.TypstSyntaxHighlighterFactory`)
    *   **Purpose:** Provides basic, lexer-based syntax highlighting. Registered via `com.intellij.lang.syntaxHighlighterFactory` extension point. LSP provides richer semantic highlighting later.
    *   **`SyntaxHighlighterFactory.getSyntaxHighlighter(project, virtualFile)`:** Returns an instance of the `SyntaxHighlighter`.
    *   **`SyntaxHighlighter.getHighlightingLexer()`:** Returns the `Lexer` instance to use for highlighting.
    *   **`SyntaxHighlighter.getTokenHighlights(tokenType)`:** Returns an array of `TextAttributesKey`s to apply to the given `IElementType`.
    *   **`TextAttributesKey`:** Defines the styling (color, font style). Can inherit from `DefaultLanguageHighlighterColors` or `HighlighterColors`.
    *   **Docs:**
        *   [Syntax and Error Highlighting](https://plugins.jetbrains.com/docs/intellij/syntax-highlighting-and-error-highlighting.html)
        *   [Custom Language Support Tutorial: Syntax Highlighter](https://plugins.jetbrains.com/docs/intellij/syntax-highlighter-and-color-settings-page.html)

## Other Relevant APIs

*   **`plugin.xml` Configuration:**
    *   **`<depends>`:** Defines mandatory dependencies on platform modules (e.g., `com.intellij.modules.platform`, `com.intellij.modules.ultimate`) or other plugins. [Plugin Dependencies](https://plugins.jetbrains.com/docs/intellij/plugin-dependencies.html)
    *   **`<extensions>`:** Registers plugin components (implementations of Extension Points). [Plugin Extension Points](https://plugins.jetbrains.com/docs/intellij/plugin-extensions.html)
    *   **Docs:** [Plugin Configuration File](https://plugins.jetbrains.com/docs/intellij/plugin-configuration-file.html)
*   **`FileType`** (`org.tinymist.intellij.TypstFileType`)
    *   **Purpose:** Associates a language with file extensions, name, description, and icon. Registered via `com.intellij.fileType` extension point.
    *   **Docs:** [File Types](https://plugins.jetbrains.com/docs/intellij/language-and-filetype.html)
*   **PSI Elements (`PsiElement`, `PsiFile`)** (`org.tinymist.intellij.TypstFile`)
    *   **Purpose:** Represent nodes in the Program Structure Interface tree, built on top of the AST. `PsiFile` is the root element for a file.
    *   **Implementation:** Often extends base classes like `PsiFileBase` or `ASTWrapperPsiElement`.
    *   **Docs:** [Implementing Parser and PSI](https://plugins.jetbrains.com/docs/intellij/implementing-parser-and-psi.html)
*   **`IElementType` / `TokenSet`** (`org.tinymist.intellij.TYPST_TEXT`)
    *   **Purpose:** `IElementType` represents the type of a token (from lexer) or an AST node (from parser). `TokenSet` groups related element types.
    *   **Docs:** [Implementing Lexer](https://plugins.jetbrains.com/docs/intellij/implementing-lexer.html), [Implementing Parser and PSI](https://plugins.jetbrains.com/docs/intellij/implementing-parser-and-psi.html)
