# Tinymist IntelliJ Plugin Development Notes

## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs. This is achieved by integrating the `tinymist` language server ([https://github.com/Myzel394/tinymist](https://github.com/Myzel394/tinymist)) into the IntelliJ Platform using the `lsp4ij` plugin developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)). The plugin aims to offer features such as syntax highlighting, autocompletion, diagnostics, hover information, go-to-definition, and potentially more, mirroring the capabilities of the Tinymist VSCode extension.


## Current Status and Next Steps

**Status as of 2025-05-07:**
*   **Phase 1: Resolve Server Startup Crash - COMPLETED**
    *   The language server (`tinymist`) now starts successfully.
*   **Phase 2: Achieve Basic Linting (Diagnostics) - COMPLETED**
    *   LSP diagnostics (errors/warnings) are correctly displayed in the editor.
*   **Phase 3: Implement Core LSP Features - MOSTLY COMPLETED**
    *   Step 1: Review/Implement `textDocument/definition` (Go To Definition) - **PARTIALLY WORKING** (Highlighting issue ON HOLD)
    *   Step 2: Review/Implement `textDocument/hover` (Hover Information) - **PARTIALLY WORKING** (Highlighting issue ON HOLD)
    *   Step 3: Update `TinymistInitializationOptions.kt` - **COMPLETED**
    *   Step 4: Review/Implement `textDocument/completion` (Code Completion) - **COMPLETED**
    *   Step 5: Review/Implement `textDocument/signatureHelp` (Signature Help) - **COMPLETED**
    *   Step 6: Review/Implement `textDocument/rename` (Rename Symbol) - **COMPLETED**
    *   Step 7: Review/Implement `textDocument/references` (Find Usages) - *TODO* (User prioritizes preview)
    *   Step 8: Others (e.g., `documentHighlight`) - *PENDING / POTENTIALLY ON HOLD* (May be related to deferred highlighting issues)

**Current Focus/Blockers:**
1.  **Go-To-Definition Highlighting:** (ON HOLD) When Ctrl/Cmd-clicking a token, the navigation works, but the entire file content is visually marked. This is likely due to [lsp4ij issue #1018](https://github.com/redhat-developer/lsp4ij/issues/1018). A fix is available in `lsp4ij` nightly builds (e.g., `0.13.0-20250506-071038`), but Gradle had issues resolving/downloading it. **Decision:** Reverted to `lsp4ij 0.12.0` for now. Will attempt to update to `lsp4ij 0.13.0` (or later) once officially released and easily consumable via Gradle.
2.  **Hover Highlighting:** (POTENTIALLY RELATED TO ABOVE) When hovering over a token, the correct information popup appears, but the token itself is not visually highlighted. This might also be resolved by the `lsp4ij` update.

**Next LSP Features to Implement (from Phase 3):**
*   `textDocument/completion` (Code Completion)
*   `textDocument/signatureHelp` (Signature Help)
*   `textDocument/references` (Find Usages)
*   `textDocument/rename` (Rename Symbol)

**Identified Technical Debt / Areas for Future Refinement:**
*   **Minimal Client-Side Parsing/Lexing:** The `TypstLexerAdapter` and `TypstParserDefinition` are currently very basic, treating the entire file as a single token. While the LSP handles detailed analysis, enhancing the client-side lexer/parser could provide:
    *   Rudimentary syntax highlighting (e.g., for keywords, comments, strings) even before the LSP initializes or if it's unavailable.
    *   Better support for IntelliJ platform features that rely on PSI structure (e.g., more accurate breadcrumbs, structure view elements not solely dependent on LSP symbols).
*   **Basic Client-Side Syntax Highlighter:** `TypstSyntaxHighlighter` provides only a single default style for all text, awaiting semantic tokens from the LSP for actual highlighting. This could be improved in conjunction with a better client-side lexer.
*   **Rudimentary LSP Executable Error Handling:** `TinymistLspStreamConnectionProvider` currently throws a `RuntimeException` if the `tinymist` executable is not found. This is planned to be improved in Phase 4 with user-friendly notifications and a link to settings, but the current state is a known issue.
*   **Missing File Type Icon:** `TypstFileType` has a `TODO` to add a dedicated file icon for `.typ` files.
*   **Hardcoded Configuration Defaults:** `TinymistInitializationOptions` uses some hardcoded default values (e.g., `colorTheme`). These will become configurable via the settings panel planned in Phase 4.
*   **Incomplete Parser Definition Features:** `TypstParserDefinition` contains `TODO` markers for defining comment and string literal token sets. Implementing these could improve basic editor interactions.
*   **Unused `_TypstLexer` Code:** The `TypstLexerAdapter.kt` file contains a `_TypstLexer` class that appears to be a remnant of a previous JFlex-based approach and is currently unused. This should be reviewed and potentially removed. - **COMPLETED**
*   **JCEF Preview Placeholder Content:** Both `TypstPreviewFileEditor` (for the split preview) and `TypstPreviewToolWindowFactory` (for the currently disabled tool window) use placeholder HTML/UI. Full integration with `tinymist`'s preview rendering is a major part of Phase 4.

**Phase 4: Implement Settings, Improve User Experience, and Advanced Features**
*   Once core LSP features are verified and stable (current state is good enough to proceed here):
    1.  **Preview Panel Integration (PRIORITY):**
        *   **Technology Choice:** JCEF (Java Chromium Embedded Framework) will be used, as it's the IntelliJ Platform standard for rendering HTML/web content (used by Markdown plugin, etc.).
        *   Plan and implement an integrated preview panel for Typst documents, likely as an IntelliJ Tool Window.
        *   This will involve handling custom LSP messages/notifications from `tinymist` like `tinymist/previewStart`, `tinymist/updatePreview`, potentially `window/showDocument` requests if the server uses them to suggest opening a preview, and custom commands like `tinymist.doStartPreview` or `tinymist.scrollPreview`.
        *   Determine how the preview content (likely HTML or SVG served by `tinymist`) will be loaded and rendered within the JCEF browser.
        *   Implement communication between plugin and JCEF JavaScript using `JBCefJSQuery` if needed for interactivity (e.g., scroll sync).

        **Typst Preview Architecture (Insights from `typst-preview-arch` and VS Code):**

        *   **Core Components:**
            *   **Typst Preview Server (Rust, part of `tinymist` LSP):** Watches files, recompiles, handles Typst world, and serves compiled/rendered data.
            *   **Typst Preview Client (Web - TypeScript/WASM):** Renders the document in a webview. Uses `typst-ts-renderer` for incremental rendering via a Virtual DOM, receiving serialized data (rkyv format) from the server via WebSockets.
            *   **Editor Extension (e.g., VS Code, our IntelliJ plugin):** Starts/manages the server, hosts the webview client, and relays editor events to the server and UI events (like theme changes) to the webview client.

        *   **Communication Flow for Rendering:**
            1.  IntelliJ Plugin (Editor Extension) requests a preview session from `tinymist` LSP.
            2.  `tinymist` LSP prepares the session and (most likely) provides a way for the plugin to load the "Typst Preview Client" (HTML/JS/WASM frontend).
                *   **Scenario 1 (LSP serves all):** `tinymist` runs an HTTP server and gives the IntelliJ plugin a URL to load in JCEF. This URL serves the `index.html` shell and all its assets (JS for `typst-ts-renderer`, WASM, CSS).
                *   **Scenario 2 (LSP provides HTML shell, IntelliJ serves assets):** `tinymist` provides an `index.html` template. The IntelliJ plugin bundles the core static assets (like `typst-ts-renderer` JS/WASM, CSS) and serves them via a local `HttpRequestHandler`. The `index.html` template's asset paths are rewritten to point to this local handler.
            3.  The JCEF panel in IntelliJ loads this `index.html` (either from `tinymist`'s URL or the plugin-massaged version).
            4.  The JavaScript (Typst Preview Client) within JCEF establishes a **WebSocket connection** directly to the `tinymist` server.
            5.  `tinymist` sends incremental rendering data (rkyv-serialized) over this WebSocket to the Typst Preview Client.
            6.  The Typst Preview Client (using `typst-ts-renderer` and WASM) deserializes this data and updates its VDOM to render the preview.

        *   **Role of IntelliJ Plugin for Preview Panel (`TypstPreviewFileEditor` with JCEF):**
            *   **Host the Webview Client:** Load the `index.html` provided or constructed as per Scenario 1 or 2 above.
            *   **Facilitate Asset Loading:** If Scenario 2, implement an `HttpRequestHandler` to serve bundled frontend assets (JS/WASM for `typst-ts-renderer`, CSS, fonts, images).
                *   **Update:** Implemented `TypstPreviewResourceHandler.kt` as a basic `HttpRequestHandler`.
                *   **Update:** Added `implementation("com.intellij.modules:platform-impl")` to `build.gradle.kts` dependencies to resolve `Unresolved reference 'Responses'` used in the resource handler.
            *   **Side-Channel Communication (via `JBCefJSQuery` - our `window.typstIntellij` bridge):
                *   **IntelliJ -> JCEF:** Send commands for theme changes (`applyTheme`), editor-initiated scrolling (`scrollToPercent`), or other UI interactions not covered by the primary WebSocket channel.
                *   **JCEF -> IntelliJ:** Receive events from the webview client like specific user interactions (e.g., clicks on special links, fine-grained scroll position for editor sync) if the client is designed to send them.
            *   **LSP Interaction:** Send requests to `tinymist` (e.g., to start a preview session, notify of editor scroll if `tinymist` needs it) and handle responses or custom notifications from `tinymist` related to the preview state.

        *   **Implications for `TypstPreviewFileEditor.updateContent()`:** The previous idea of pushing full HTML snippets via `updateContent()` is likely incorrect for the main Typst rendering, as this is handled via the WebSocket and `typst-ts-renderer`. `updateContent()` might be repurposed for overlays or status info if needed, or removed for rendering purposes.

        **API/Pattern Insights for JCEF Preview Panels (from Markdown Plugin Reference):**

        *   **Editor Structure for Text + Preview:**
            *   Utilize `com.intellij.openapi.fileEditor.TextEditorWithPreviewProvider`: This IntelliJ Platform class is designed to create a side-by-side editor (standard text editor + a custom preview editor).
            *   The provider is registered in `plugin.xml` for the relevant file type(s).
            *   It requires a secondary `FileEditorProvider` that creates the custom preview `FileEditor`.
            *   The custom `FileEditor` implementation will be responsible for hosting and managing the JCEF browser component.

        *   **HTML Rendering Panel Abstraction (Optional but good practice):**
            *   Define an interface (e.g., `MyHtmlPanel`) that outlines the contract for any HTML rendering component. This promotes flexibility if different rendering strategies (e.g., different JCEF configurations, fallbacks) are ever needed.
            *   Key methods for such an interface typically include:
                *   `getComponent(): JComponent`: Returns the Swing UI component of the panel.
                *   `setHtml(htmlContent: String, baseUrlForResources: String?)`: To load/update the HTML. The `baseUrlForResources` is crucial for resolving relative paths (images, CSS) within the loaded HTML.
                *   Mechanisms for JS <-> Kotlin communication (e.g., exposing a message-passing object).
                *   Methods for controlling scroll position.
            *   If multiple panel implementations are offered, an extension point (like `com.intellij.openapi.extensions.ExtensionPointName`) can be used to discover them, and settings can allow the user to choose.

        *   **JCEF-Based Panel Implementation (e.g., `MyJCEFHtmlPanel` extending `com.intellij.ui.jcef.JCEFHtmlPanel`):**
            *   **Core Component**: The panel will extend `com.intellij.ui.jcef.JCEFHtmlPanel` (or directly use `com.intellij.ui.jcef.JBCefBrowser`).
            *   **Serving Local Static Assets (Base HTML, CSS, JS):**
                *   If the preview requires a shell HTML page, custom CSS, or JavaScript files bundled with the plugin, these can be served using a custom `org.jetbrains.ide.HttpRequestHandler`.
                *   This handler integrates with IntelliJ's built-in lightweight HTTP server.
                *   It typically defines a unique URL prefix for the plugin (e.g., `http://localhost:<port>/myplugin_preview_static/...`).
                *   It needs a way to map requested paths to actual plugin resources (e.g., files in `src/main/resources`).
            *   **Loading Content into JCEF:**
                *   **Scenario 1 (Language server provides URL):** If the language server (like `tinymist`) serves the preview content via its own HTTP endpoint, `JBCefBrowser.loadURL(...)` can be used directly.
                *   **Scenario 2 (Language server provides HTML string):** Use `JBCefBrowser.loadHTML(htmlContent, baseUrlForResources)`. The `baseUrlForResources` (can be a `file:///` URL pointing to the document's directory or a URL from the local static server) is critical for resolving relative paths (images, etc.) in the HTML.
                *   **Scenario 3 (Plugin-hosted shell + dynamic content):** Load a base HTML page from the local static server (if used), then use JavaScript (executed via `JBCefBrowser.executeJavaScript(...)`) to inject or update content dynamically. More advanced techniques (like virtual/incremental DOM patching via JS) can be used for performance but add complexity.
            *   **Kotlin <-> JavaScript Communication:**
                *   `com.intellij.ui.jcef.JBCefJSQuery` is the standard mechanism for two-way communication.
                *   Define a clear message protocol (e.g., JSON strings with `type` and `payload` fields).
                *   Kotlin side: Create `JBCefJSQuery` and add a handler to process messages from JS.
                *   JS side: Use the function injected by `JBCefJSQuery` (available via `window.cefQuery(...)` or a custom name) to send messages to Kotlin. To receive messages from Kotlin, Kotlin calls `JBCefBrowser.executeJavaScript(...)` to invoke specific JS functions.
            *   **Styling and Theming:**
                *   Serve custom CSS files via the local static server or inject CSS strings using `executeJavaScript`.
                *   Listen to IntelliJ theme changes (e.g., `com.intellij.ide.ui.UISettingsListener`) and use `executeJavaScript` to update CSS variables, apply/remove theme-specific classes, or re-load stylesheets in the JCEF panel.
            *   **Resource Handling for Relative Paths:** Correctly set the base URL when loading HTML (via `loadHTML` or by loading from a specific URL) so that JCEF can resolve relative paths for images, fonts, etc. The local static server can be made to serve these resources from the project or plugin.

        *   **Scroll Synchronization:**
            *   Implement JS listeners for scroll events in the JCEF panel. These listeners use `JBCefJSQuery` to send the current scroll position/percentage to Kotlin.
            *   Implement Kotlin functions that, when triggered (e.g., editor scroll, LSP request), call `JBCefBrowser.executeJavaScript(...)` to invoke a JS function that scrolls the JCEF panel to the desired location.

    2.  **Implement IntelliJ Settings Panel:**
        *   Create a dedicated settings/preferences page for Tinymist (e.g., under "Languages & Frameworks" or "Tools").
        *   Allow configuration of: Path to the `tinymist` executable, font paths, PDF export settings, preview-related settings, and other relevant options derived from `TinymistConfig` (VSCode) and `TinymistInitializationOptions`.
    3.  **Load Settings into `TinymistInitializationOptions`:**
        *   In `TinymistLspStreamConnectionProvider#getInitializationOptions`, retrieve configured values from the settings panel and correctly populate the `TinymistInitializationOptions` data class.
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
