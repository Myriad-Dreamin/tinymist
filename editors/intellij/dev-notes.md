# Tinymist IntelliJ Plugin Development Notes

## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs. This is achieved by integrating the `tinymist` language server ([https://github.com/Myzel394/tinymist](https://github.com/Myzel394/tinymist)) into the IntelliJ Platform using the `lsp4ij` plugin developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)). The plugin aims to offer features such as syntax highlighting, autocompletion, diagnostics, hover information, go-to-definition, and potentially more, mirroring the capabilities of the Tinymist VSCode extension.

## Project Roadmap & Status

### I. Completed Milestones
*   **Initial Server Integration:** Resolved server startup crashes.
*   **Basic Diagnostics:** Implemented linting/diagnostics.
*   **Core LSP Features (Majority):**
    *   `textDocument/completion` (Code Completion)
    *   `textDocument/signatureHelp` (Signature Help)
    *   `textDocument/rename` (Rename Symbol)
*   **Configuration:** Updated `TinymistInitializationOptions.kt` to support `preview.background.enabled` for `tinymist`'s preview server.
*   **Preview Strategy Foundation:** Validated that `tinymist` LSP starts its background preview server and `TypstPreviewFileEditor` loads content from it (e.g., `http://127.0.0.1:23635`).

### II. Current Focus & Active Debugging
*   **Preview Panel Scrolling Performance:**
    *   **Issue:** Significant scrolling lag/input delay in the JCEF-based preview panel. Lag is affected by JCEF DevTools/FPS meter.
    *   **Active Investigation (User):** Resolving `npm install` errors in `tools/typst-preview-frontend` to enable `yarn build:preview`. Goal is to rebuild `tinymist` with an instrumented frontend to capture detailed performance logs related to scroll event handling.
    *   **Frontend Build Workflow Confirmed:**
        1.  Build frontend: `yarn build:preview` (from `tinymist` root) -> copies `typst-preview.html` to `crates/tinymist-assets/src/`.
        2.  Configure `tinymist/Cargo.toml`: Use local path for `tinymist-assets` (`tinymist-assets = { path = "./crates/tinymist-assets/" }`).
        3.  Rebuild `tinymist`: `cargo build`.

### III. On Hold / Blocked Tasks
*   **Preview Panel Scrolling Performance (Further Frontend Debugging - PAUSED):**
    *   **Reason:** Awaiting feedback/input on the drafted GitHub issue for `tinymist` maintainers.
    *   **Issue Summary for GitHub:**
        *   Title: Scrolling Input Lag / Jumpy Behavior in Typst Preview Frontend (Observed in Embedded Browser View)
        *   Key Points: Lag affected by DevTools, `processMessage` is fast, reducing scroll `debounceTime` helps but doesn't fully fix.
        *   Questions: Other frontend delays? `TypstDocument.addViewportChange()` internals? `throttleTime` vs `debounceTime`? Why DevTools alters behavior?
*   **`textDocument/definition` (Go To Definition):** Partially working; highlighting issue.
*   **`textDocument/hover` (Hover Information):** Partially working; highlighting issue (potentially related to Go-To-Definition).
*   **`documentHighlight` (Other LSP Features):** Pending.

### IV. Immediate Next Steps (High Priority - Post Current Debugging)
*   **`textDocument/references` (Find Usages):** Implement this core LSP feature.
*   **Stabilize Preview Panel Integration:**
    *   Based on feedback from the GitHub issue and potential fixes, ensure smooth and reliable preview rendering and interaction.
    *   Refine LSP interaction for preview if needed (e.g., scroll sync, theme changes via `JBCefJSQuery`).

### V. Planned Features & Enhancements (Longer Term)
*   **IntelliJ Settings Panel:**
    *   Configure path to `tinymist` executable.
    *   Configure font paths, PDF export options.
    *   Settings for `tinymist` preview server (e.g., host/port, if configurable beyond `preview.background.enabled`).
*   **Robust `tinymist` Executable Handling:**
    *   Prioritize configured path in settings for `findTinymistExecutable()`.
    *   Fall back to searching `PATH`.
    *   User-friendly notifications if not found (balloon notification with link to settings).
    *   Consider bundling `tinymist` or providing clear download/setup instructions.
*   **Full Server-Specific Interactions:**
    *   Systematically implement robust handlers for: `workspace/configuration` requests, `textDocument/didOpen|Change|Close` for auxiliary files, focus tracking notifications.
*   **Documentation:**
    *   Update plugin `README.md` (setup, features, settings).
    *   Keep `dev-notes.md` current.

### VI. Technical Debt & Refinements
*   Minimal Client-Side Parsing/Lexing (Evaluate if still relevant with LSP).
*   Basic Client-Side Syntax Highlighter (Evaluate if still relevant with LSP semantic tokens).
*   Rudimentary LSP Executable Error Handling (Partially addressed by "Robust `tinymist` Executable Handling" above).
*   Missing File Type Icon.
*   Hardcoded Configuration Defaults in `TinymistInitializationOptions` (e.g., `colorTheme`, preview URL - review what should be settings).
*   Incomplete Parser Definition Features (Evaluate if still relevant).
*   JCEF Preview Placeholder Content: Largely addressed as `tinymist` serves its own UI.

### VII. Preview Architecture Notes (Reference)

*   **Strategy:** Leverage `tinymist`'s built-in preview server.
*   **IntelliJ Plugin Role:**
    *   Pass `preview.background.enabled = true` in `TinymistInitializationOptions`.
    *   `TypstPreviewFileEditor` hosts JCEF browser, loads URL from `tinymist`'s preview server.
    *   Plugin does NOT serve its own static assets for preview.
*   **Communication:**
    *   JCEF client JS establishes WebSocket to `tinymist` server for rendering updates.
    *   `JBCefJSQuery` for side-channel communication (theme, scroll sync) if needed.
*   `TypstPreviewFileEditor.updateContent()`: Confirmed unnecessary for main rendering.


## Project Architecture and File Overview

This section outlines the architecture of the Tinymist IntelliJ plugin, detailing the roles of key files and their interactions, particularly with the IntelliJ Platform and LSP4IJ APIs.

### Core Directory Structure

*   **`editors/intellij/`**: Root directory for the IntelliJ plugin.
    *   **`build.gradle.kts`**: Gradle build script for managing dependencies (like `lsp4ij`, IntelliJ Platform SDK) and plugin packaging.
    *   **`src/main/kotlin/org/tinymist/intellij/`**: Contains the core Kotlin source code for the plugin.
    *   **`src/main/resources/META-INF/plugin.xml`**: The plugin descriptor file, essential for IntelliJ to load and recognize the plugin and its components.

### Kotlin Source Files (`src/main/kotlin/org/tinymist/intellij/`)

1.  **Basic Language Definition:**
    *   **`TypstLanguage.kt`**: Defines the `TypstLanguage` object (subclass of `com.intellij.lang.Language`) and `TypstFileType` object (subclass of `com.intellij.openapi.fileTypes.LanguageFileType`). This is the most basic registration of "Typst" as a language within IntelliJ.
    *   **`TypstFile.kt`**: Defines `TypstFile` (subclass of `com.intellij.extapi.psi.PsiFileBase`), representing a Typst file in the PSI (Program Structure Interface) tree.

2.  **Lexing and Parsing (Minimal Implementation):**
    *   **`TypstLexerAdapter.kt`**: Implements `com.intellij.lexer.Lexer`. Provides a very basic lexer that treats the entire file content as a single token (`TYPST_TEXT`). This is a placeholder as the actual detailed lexing and parsing for features like syntax highlighting and code analysis are delegated to the `tinymist` LSP server.
    *   **`TypstParserDefinition.kt`**: Implements `com.intellij.lang.ParserDefinition`.
        *   Returns the `TypstLexerAdapter`.
        *   Provides a basic `PsiParser` that creates a single root PSI node for the file. Again, this is minimal because the LSP server handles the heavy lifting of understanding the code structure.
        *   Defines how to create a `TypstFile` PSI element.
        *   Defines `TYPST_TEXT` as an `IElementType`.
    *   **`TypstSyntaxHighlighter.kt` & `TypstSyntaxHighlighterFactory.kt`**:
        *   `TypstSyntaxHighlighterFactory` implements `com.intellij.openapi.fileTypes.SyntaxHighlighterFactory` and provides instances of `TypstSyntaxHighlighter`.
        *   `TypstSyntaxHighlighter` (subclass of `com.intellij.openapi.fileTypes.SyntaxHighlighterBase`) uses the `TypstLexerAdapter`. It assigns a default text attribute to the `TYPST_TEXT` token. Actual rich syntax highlighting is expected to come from the LSP server via semantic token support.

3.  **LSP (Language Server Protocol) Integration (`lsp/` directory):**
    *   **`TinymistLanguageServerFactory.kt`**: Implements `com.redhat.devtools.lsp4ij.LanguageServerFactory`. Its primary role is to create and provide an instance of the `StreamConnectionProvider` for the Tinymist language server. It instantiates `TinymistLspStreamConnectionProvider`. This factory is registered in `plugin.xml`.
    *   **`TinymistLspStreamConnectionProvider.kt`**: Extends `com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider`. This is a crucial class for managing the lifecycle and communication with the `tinymist` LSP executable.
        *   In its `init` block, it calls `findTinymistExecutable()` to locate the `tinymist` binary on the system's PATH.
        *   It then uses `super.setCommands()` to configure the command to start the server (e.g., `["path/to/tinymist", "lsp"]`).
        *   `getWorkingDirectory()`: Returns the project's base path as the working directory for the LSP server.
        *   `getInitializationOptions()`: Constructs and returns a `TinymistInitializationOptions` object. This object is serialized to JSON and sent to the LSP server as part of the `initialize` request. It allows passing client-specific configurations to the server on startup.
    *   **`TinymistInitializationOptions.kt`**: A Kotlin data class that defines the structure of the initialization options sent to the `tinymist` server. It includes fields like `font.fontPaths`, `semanticTokens`, `completion`, `lint`, etc., mirroring configurations available in the Tinymist VSCode extension.

4.  **Preview Panel (`preview/` directory):**
    *   **`TypstPreviewFileEditor.kt`**: Implements `com.intellij.openapi.fileEditor.FileEditor`. This class is responsible for rendering the Typst preview.
        *   It uses `com.intellij.ui.jcef.JBCefBrowser` to embed a Chromium-based browser panel.
        *   It currently loads placeholder HTML but has methods like `updateContent(html: String)` and `loadURL(url: String)` which will be used to display the actual preview content received from or served by the `tinymist` server.
        *   Contains a nested `Provider` class (`TypstPreviewFileEditor.Provider`) which implements `com.intellij.openapi.fileEditor.FileEditorProvider`. This nested provider is used by `TypstTextEditorWithPreviewProvider`.
    *   **`TypstPreviewFileEditorProvider.kt`**: Defines `TypstTextEditorWithPreviewProvider` which extends `com.intellij.openapi.fileEditor.TextEditorWithPreviewProvider`. This is the main mechanism for showing a side-by-side view of the Typst text editor and its preview.
        *   It is registered in `plugin.xml` as a `fileEditorProvider`.
        *   It takes an instance of `TypstPreviewFileEditor.Provider()` in its constructor to create the preview part of the editor.
        *   `accept()`: Ensures this provider is only used for Typst files.
    *   **`TypstPreviewToolWindowFactory.kt`**: Implements `com.intellij.openapi.wm.ToolWindowFactory`. It's designed to create a separate "Typst Preview" tool window.
        *   Its registration in `plugin.xml` is currently commented out, suggesting the `TextEditorWithPreviewProvider` is the preferred method for preview.
        *   The current implementation creates a simple JPanel with a placeholder label.

### Resources (`src/main/resources/`)

*   **`META-INF/plugin.xml`**: The plugin descriptor. This XML file declares the plugin's existence and its components to the IntelliJ Platform. Key declarations include:
    *   Plugin ID, name, version, description, and dependencies (e.g., `com.redhat.devtools.lsp4ij`).
    *   **`<extensions defaultExtensionNs="com.intellij">`**:
        *   `fileType`: Associates `.typ` extension with `TypstFileType` and `TypstLanguage`.
        *   `lang.parserDefinition`: Registers `TypstParserDefinition` for `TypstLanguage`.
        *   `lang.syntaxHighlighterFactory`: Registers `TypstSyntaxHighlighterFactory` for `TypstLanguage`.
        *   `fileEditorProvider`: Registers `TypstTextEditorWithPreviewProvider` to enable the split text/preview editor for Typst files.
    *   **`<extensions defaultExtensionNs="com.redhat.devtools.lsp4ij">`**:
        *   `server`: Defines the "tinymistServer", specifying `TinymistLanguageServerFactory` as its factory.
        *   `fileNamePatternMapping`: Maps `*.typ` files to the "tinymistServer", enabling LSP features for these files.

### API Interactions Summary

*   **IntelliJ Platform API**:
    *   **Language Support**: `Language`, `LanguageFileType`, `PsiFileBase`, `ParserDefinition`, `Lexer`, `PsiParser`, `SyntaxHighlighterFactory`, `SyntaxHighlighterBase`. These are used to provide basic recognition of the Typst language, though most heavy lifting is offloaded to the LSP.
    *   **File System & Project Model**: `Project`, `VirtualFile`.
    *   **Editors**: `FileEditor`, `FileEditorProvider`, `TextEditorWithPreviewProvider`. Used for creating the text editor and the preview panel.
    *   **UI (JCEF)**: `JBCefApp`, `JBCefBrowser` for embedding the web-based preview.
    *   **Tool Windows**: `ToolWindowFactory` (though currently not the primary preview mechanism).
    *   **Plugin Descriptor (`plugin.xml`)**: Defines extension points to integrate custom components.
*   **LSP4IJ API (`com.redhat.devtools.lsp4ij`)**:
    *   `LanguageServerFactory`: Implemented by `TinymistLanguageServerFactory` to integrate the LSP.
    *   `ProcessStreamConnectionProvider`: Extended by `TinymistLspStreamConnectionProvider` to manage the external `tinymist` process.
    *   LSP4IJ handles the general LSP message passing (JSON-RPC) between IntelliJ and the `tinymist` server, translating LSP notifications and requests into IntelliJ actions (e.g., displaying diagnostics, showing completion items).
*   **Tinymist LSP Server (External Process)**:
    *   Communicates via standard input/output using the Language Server Protocol.
    *   Receives initialization options (`TinymistInitializationOptions`).
    *   Provides semantic information: diagnostics, completions, hover info, go-to-definition, semantic highlighting, etc.
    *   Expected to provide HTML/SVG content for the preview panel, or an HTTP endpoint from which the JCEF browser can load the preview. Interactions like `tinymist/previewStart`, `tinymist/updatePreview` will be handled by custom LSP message handlers (to be implemented or refined).

This architecture relies heavily on LSP4IJ to bridge the IntelliJ Platform with the `tinymist` language server, allowing the plugin to focus on specific integrations like the JCEF preview and user settings, while leveraging `tinymist` for core language intelligence.
