# Tinymist IntelliJ Plugin Development Notes

## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs. This is achieved by integrating the `tinymist` language server ([https://github.com/Myzel394/tinymist](https://github.com/Myzel394/tinymist)) into the IntelliJ Platform using the `lsp4ij` plugin developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)). The plugin aims to offer features such as syntax highlighting, autocompletion, diagnostics, hover information, go-to-definition, and potentially more, mirroring the capabilities of the Tinymist VSCode extension.


## Current Status and Next Steps

**Status as of 2025-05-07 (updated for new preview strategy):**
*   **Phase 1: Resolve Server Startup Crash - COMPLETED**
*   **Phase 2: Achieve Basic Linting (Diagnostics) - COMPLETED**
*   **Phase 3: Implement Core LSP Features - MOSTLY COMPLETED**
    *   Step 1: Review/Implement `textDocument/definition` (Go To Definition) - **PARTIALLY WORKING** (Highlighting issue ON HOLD)
    *   Step 2: Review/Implement `textDocument/hover` (Hover Information) - **PARTIALLY WORKING** (Highlighting issue ON HOLD)
    *   Step 3: Update `TinymistInitializationOptions.kt` - **COMPLETED** (Now includes `preview.background.enabled`)
    *   Step 4: Review/Implement `textDocument/completion` (Code Completion) - **COMPLETED**
    *   Step 5: Review/Implement `textDocument/signatureHelp` (Signature Help) - **COMPLETED**
    *   Step 6: Review/Implement `textDocument/rename` (Rename Symbol) - **COMPLETED**
    *   Step 7: Review/Implement `textDocument/references` (Find Usages) - *TODO* (User prioritizes preview)
    *   Step 8: Others (e.g., `documentHighlight`) - *PENDING / POTENTIALLY ON HOLD*

**Current Focus/Blockers:**
1.  **Go-To-Definition Highlighting:** (ON HOLD)
2.  **Hover Highlighting:** (POTENTIALLY RELATED TO ABOVE)
3.  **Preview Panel Integration:** Validating that `tinymist` LSP starts its background preview server when `preview.background.enabled=true` is passed via `TinymistInitializationOptions` and that `TypstPreviewFileEditor` correctly loads the content from `http://127.0.0.1:23635`.

**Next LSP Features to Implement (from Phase 3):**
*   `textDocument/references` (Find Usages)

**Identified Technical Debt / Areas for Future Refinement:**
*   **Minimal Client-Side Parsing/Lexing**
*   **Basic Client-Side Syntax Highlighter**
*   **Rudimentary LSP Executable Error Handling**
*   **Missing File Type Icon**
*   **Hardcoded Configuration Defaults:** `TinymistInitializationOptions` (e.g., `colorTheme`, preview URL).
*   **Incomplete Parser Definition Features**
*   **JCEF Preview Placeholder Content (Largely addressed):** `TypstPreviewFileEditor` now loads a dynamic URL from `tinymist`. The placeholder aspect is resolved if `tinymist` serves its full UI.

**Phase 4: Implement Settings, Improve User Experience, and Advanced Features**
*   Once core LSP features and the new preview integration are verified and stable:
    1.  **Preview Panel Integration (PRIORITY - New Approach):**
        *   **Strategy:** Leverage `tinymist`'s built-in preview server. The plugin will configure `tinymist` (via `TinymistInitializationOptions`) to start its background preview server (e.g., `tinymist preview --data-plane-host=127.0.0.1:23635 --invert-colors=auto`).
        *   **IntelliJ Plugin Role:**
            *   Pass `preview.background.enabled = true` (and potentially other preview args like default host/port if made configurable) in `TinymistInitializationOptions`.
            *   `TypstPreviewFileEditor` will host a JCEF browser.
            *   The JCEF browser will load the URL where `tinymist` serves its preview (e.g., `http://127.0.0.1:23635`). `tinymist` is expected to serve all necessary HTML, JS, WASM, and CSS assets for its preview client.
            *   The plugin will no longer serve its own static assets for the preview via `TypstPreviewResourceHandler.kt` (which has been removed).
        *   **LSP Interaction for Preview:** Continue to handle custom LSP messages/notifications from `tinymist` (e.g., `tinymist/updatePreview`, `tinymist.scrollPreview`) for interactivity like scroll sync, theme changes, etc., using `JBCefJSQuery` for communication between the JCEF panel and the plugin.

        **Typst Preview Architecture (Updated Insights):**

        *   **Core Components:** Remains largely the same (Tinymist Preview Server, Typst Preview Client, Editor Extension).
        *   **Communication Flow for Rendering (Simplified for IntelliJ Plugin):**
            1.  IntelliJ Plugin sends `initialize` request to `tinymist` LSP with `preview.background.enabled = true`.
            2.  `tinymist` LSP starts/manages its own `tinymist preview` server, which listens on a known port (e.g., `127.0.0.1:23635`) and serves its complete web-based preview client (HTML, JS, WASM, CSS).
            3.  The JCEF panel in `TypstPreviewFileEditor` in IntelliJ loads the URL from the `tinymist` preview server.
            4.  The JavaScript (Typst Preview Client) within JCEF establishes a WebSocket connection directly to the `tinymist` server for dynamic rendering updates.
            5.  (No change here) `tinymist` sends incremental rendering data.
            6.  (No change here) Typst Preview Client renders updates.

        *   **Role of IntelliJ Plugin for Preview Panel (`TypstPreviewFileEditor` with JCEF - Revised):**
            *   **Host the Webview Client:** Load the URL served by `tinymist`'s background preview server (e.g., `http://127.0.0.1:23635`).
            *   **No Asset Serving by Plugin:** The plugin no longer needs to implement an `HttpRequestHandler` (like the removed `TypstPreviewResourceHandler.kt`) to serve preview assets.
            *   **Side-Channel Communication (via `JBCefJSQuery`):** Still relevant for theme changes, scroll sync, etc., if `tinymist` expects these via custom messages/commands.
            *   **LSP Interaction:** Still relevant for managing the preview lifecycle if `tinymist` uses custom commands/notifications beyond the initial startup.

        *   **Implications for `TypstPreviewFileEditor.updateContent()`:** This method is confirmed to be unnecessary for rendering the main preview content, as this is handled by `tinymist` serving its web application and subsequent WebSocket communication.

        **API/Pattern Insights for JCEF Preview Panels (from Markdown Plugin Reference - Adjusted):**

        *   **Editor Structure for Text + Preview:** Unchanged.
        *   **HTML Rendering Panel Abstraction:** Less critical if `TypstPreviewFileEditor` directly uses `JBCefBrowser.loadURL()`.
        *   **JCEF-Based Panel Implementation:**
            *   **Core Component**: Unchanged (`JBCefBrowser`).
            *   **Serving Local Static Assets:** This is **NO LONGER APPLICABLE** for the core preview, as `tinymist` serves its own assets. The plugin does not need its own `HttpRequestHandler` for this.
            *   **Loading Content into JCEF:**
                *   **Scenario (Primary):** Language server (`tinymist`) provides a URL. `JBCefBrowser.loadURL(...)` is used. This is our current approach.
            *   **Kotlin <-> JavaScript Communication:** Still relevant for advanced interactivity via `JBCefJSQuery`.
            *   **Styling and Theming:** Interactions for theming would likely be commands sent to the JCEF JS environment to ask `tinymist`'s client to adjust its theme, or `tinymist` might observe system theme via browser capabilities.
            *   **Resource Handling for Relative Paths:** Handled by the `tinymist` server.

        *   **Scroll Synchronization:** Unchanged (via `JBCefJSQuery` and potentially custom LSP messages).

    2.  **Implement IntelliJ Settings Panel:**
        *   Allow configuration of: Path to `tinymist`, font paths, PDF export, preview-related settings (e.g., if `tinymist` allows configuring its preview server port or behavior via `TinymistInitializationOptions` beyond just `preview.background.enabled`).
    3.  **Load Settings into `TinymistInitializationOptions`:**
        *   Populate `previewBackgroundEnabled` and any other `tinymist` preview args from settings.
    4.  **Enhance `findTinymistExecutable()` in `TinymistLspStreamConnectionProvider` & Error Handling:**
        *   Modify the `init` block of `TinymistLspStreamConnectionProvider` (and `findTinymistExecutable`) to:
        *   Prioritize the path configured in IntelliJ settings.
        *   Fall back to searching `PATH`.
            *   If the executable is not found or invalid, display a user-friendly IntelliJ notification (e.g., a balloon notification with a link to settings) instead of throwing a `RuntimeException`. Prevent LSP connection attempts if the path is invalid.
        *   Consider options for bundling `tinymist` or providing clear download/setup instructions within the settings UI.
    5.  **Full Implementation of Server-Specific Interactions:**
        *   Systematically implement robust handlers for: `workspace/configuration` requests, sending `textDocument/didOpen|Change|Close` for auxiliary files, and focus tracking notifications, based on a deeper understanding of `tinymist`'s requirements.
    6.  **Documentation:**
        *   Update the plugin's `README.md` with setup instructions, feature overview, and settings guide.
        *   Ensure these development notes (`PLUGIN_DEV_NOTES.md`) are kept up-to-date.

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
