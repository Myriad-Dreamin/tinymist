# Tinymist IntelliJ Plugin Development Notes
> Last Updated: August 31, 2025


## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs.
We are using the `lsp4ij` library developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)).

## Development Instructions

1.  **Prerequisites:**
    *   **IntelliJ IDEA:** Other IDEs will also work, but given developing a plugin for IntelliJ, the best support for that  is provided by IntelliJ.
    *   **JDK 21**


2.  **Clone the Repository:**
    ```bash
    git clone https://github.com/Myriad-Dreamin/tinymist.git
    cd tinymist/editors/intellij
    ```

3.  **Open in IntelliJ IDEA:**
    *   Open IntelliJ IDEA.
    *   Select "Open" and navigate to the `editors/intellij` directory of the cloned repository.
    *   IntelliJ should automatically recognize it as a Gradle project. If not, you might need to import it as a Gradle project.


4.  **Build the Plugin:**
    *   Wait for Gradle sync to complete and download all dependencies.
    *   You can build the plugin using the Gradle tool window in IntelliJ (Tasks > intellij > buildPlugin) or via the terminal:
        ```bash
        ./gradlew buildPlugin
        ```

5.  **Run/Debug:**
    *   Use the Gradle task `runIde` (Tasks > intellij > runIde) from the Gradle tool window or terminal:
        ```bash
        ./gradlew runIde
        ```
    *   This will launch a blank IntelliJ IDEA instance with the Tinymist plugin installed.
    *   You can then create or open a Typst project/file in this sandbox environment to test the plugin's features.
    *   Standard debugging tools (breakpoints, etc.) can be used in your main IntelliJ IDEA instance where the plugin code is open.

6.  **`tinymist` Language Server Path:**
    *   Ensure that `tinymist` is installed on your system and the path in `TinymistLspStreamConnectionProvider.kt` is correct for your development environment if you are modifying the LSP.

7.  **Viewing Logs:**
    *   **IntelliJ Plugin Logs:** Check the `idea.log` file of the sandboxed IntelliJ instance. You can find its location via "Help" > "Show Log in Finder/Explorer" in the sandbox IDE.
    *   **LSP Communication Logs:** `lsp4ij` provides an "LSP Consoles" view in the sandbox IDE (usually accessible from the tool window bar at the bottom left). Set its verbosity (e.g., to "verbose") via `Languages & Frameworks > Language Servers` settings to see JSON-RPC messages between the plugin and `tinymist`.


## Project Roadmap & Status

### I. Completed Milestones
*   **Initial Server Integration:** Resolved server startup crashes.
*   **Basic Diagnostics:** Implemented linting/diagnostics with custom formatting.
*   **Core LSP Features:**
    *   `textDocument/completion` (Code Completion) - Fully implemented and tested
    *   `textDocument/hover` (Hover Information) - Fully implemented and tested
    *   `textDocument/definition` (Go To Definition) - Fully implemented and tested
    *   `textDocument/signatureHelp` (Signature Help) - Implemented
    *   `textDocument/rename` (Rename Symbol) - Implemented
*   **Configuration:** Robust executable path resolution with settings integration
*   **Preview Integration:** Full JCEF-based preview with tinymist's background preview server
*   **Settings Panel:** Comprehensive settings panel with server management modes (auto-install vs custom path)
*   **Automated Server Installation:** Full cross-platform auto-installation system for tinymist binaries
*   **Server Management:** Dual-mode server management (AUTO_MANAGE for auto-installation, CUSTOM_PATH for manual configuration)

## LSP Features Implementation Status

The following table shows the implementation status of LSP features as supported by the tinymist server:

| LSP Feature                                | Status               | Implementation Type   | Notes                                                                                                                            |
|--------------------------------------------|----------------------|-----------------------|----------------------------------------------------------------------------------------------------------------------------------|
| `textDocument/completion`                  | ✅ Implemented        | Handled by lsp4ij     | Auto-completion for Typst syntax and functions                                                                                   |
| `textDocument/hover`                       | ✅ Implemented        | Handled by lsp4ij     | Documentation and type information on hover                                                                                      |
| `textDocument/definition`                  | ✅ Implemented        | Handled by lsp4ij     | Go to definition functionality                                                                                                   |
| `textDocument/signatureHelp`               | ✅ Implemented        | Handled by lsp4ij     | Function signature hints                                                                                                         |
| `textDocument/rename`                      | ✅ Implemented        | Handled by lsp4ij     | Symbol renaming                                                                                                                  |
| `textDocument/publishDiagnostics`          | ✅ Implemented        | Direct implementation | Custom diagnostic formatting with HTML support (TinymistLanguageClient.kt:24)                                                    |
| `textDocument/semanticTokens`              | ✅ Implemented        | Handled by lsp4ij     | Semantic syntax highlighting                                                                                                     |
| `textDocument/references`                  | ✅ Implemented        | Handled by lsp4ij     | Find all references to a symbol                                                                                                  |
| `textDocument/documentHighlight`           | ✅ partly implemented | Handled by lsp4ij     | Highlight related symbols; currently the highlight only works upon entirely selecting a symbol not just placing the carret there |
| `textDocument/documentSymbol`              | ✅ Implemented        | Handled by lsp4ij     | Document outline/structure view                                                                                                  |
| `textDocument/inlayHint`                   | ✅ Implemented        | Handled by lsp4ij     | Inlay additional information into code editor, i.e. the names of function parameters                                             |
| `textDocument/codeAction`                  | ✅ Implemented        | Handled by lsp4ij     | Code fixes and refactoring actions                                                                                               |
| `textDocument/formatting`                  | ❌ Not implemented    | -                     | Document formatting                                                                                                              |
| `textDocument/rangeFormatting`             | ❌ Not implemented    | -                     | Range-based formatting                                                                                                           |
| `textDocument/onTypeFormatting`            | ❌ Not implemented    | -                     | Format-on-type                                                                                                                   |
| `textDocument/codeLens`                    | ❌ Not implemented    | -                     | Inline code annotations                                                                                                          |
| `textDocument/foldingRange`                | ✅ Implemented        | Handled by lsp4ij     | Code folding regions                                                                                                             |
| `textDocument/selectionRange`              | ✅ Implemented        | Handled by lsp4ij     | Smart text selection                                                                                                             |
| `textDocument/prepareCallHierarchy`        | ❌ Not implemented    | -                     | Call hierarchy preparation                                                                                                       |
| `textDocument/callHierarchy/incomingCalls` | ❌ Not implemented    | -                     | Incoming call hierarchy                                                                                                          |
| `textDocument/callHierarchy/outgoingCalls` | ❌ Not implemented    | -                     | Outgoing call hierarchy                                                                                                          |
| `textDocument/linkedEditingRange`          | ❌ Not implemented    | -                     | Linked editing of related symbols                                                                                                |
| `textDocument/moniker`                     | ❌ Not implemented    | -                     | Symbol monikers for cross-references                                                                                             |
| `workspace/didChangeConfiguration`         | ✅ Implemented        | Handled by lsp4ij     | Configuration change notifications                                                                                               |
| `workspace/didChangeWatchedFiles`          | ✅ Implemented        | Handled by lsp4ij     | File watching                                                                                                                    |
| `workspace/symbol`                         | ✅ Implemented        | Handled by lsp4ij     | Workspace-wide symbol search                                                                                                     |
| `window/showMessage`                       | ✅ Implemented        | Handled by lsp4ij     | Server messages to client                                                                                                        |
| `window/showMessageRequest`                | ✅ Implemented        | Handled by lsp4ij     | Message request handling                                                                                                         |
| `tinymist/document`                        | ✅ Implemented        | Direct implementation | Custom tinymist notification (TinymistLanguageClient.kt:58)                                                                      |
| `tinymist/documentOutline`                 | ✅ Not implemented    | Direct implementation | Custom outline notification (TinymistLSPDiagnosticFeature)                                                                       |

### Legend:
- **✅ Implemented**: Feature is working and available
- **❌ Not implemented**: Feature is not yet implemented in the plugin
- **Handled by lsp4ij**: Feature implementation is provided by the lsp4ij library
- **Direct implementation**: Feature has custom implementation in the plugin code

### II. Current Focus & Next Steps
*   **Additional LSP Features:** Implement remaining LSP features like `textDocument/references`, `textDocument/codeAction`, and `textDocument/formatting`
*   **Preview Panel Stability:** Continue monitoring and addressing any JCEF preview panel issues that may arise

### III. Next Steps
*   **Additional LSP Features:** Implement `textDocument/codeAction`, `textDocument/formatting`, and other remaining features
*   **Preview Panel Enhancements:** Continue improving preview functionality and addressing any performance issues

### V. Planned Features & Enhancements
*   **Additional LSP Features:**
    *   `textDocument/codeAction` (Code fixes and refactoring actions)
    *   `textDocument/formatting` (Document formatting)
*   **Enhanced Settings Panel:**
    *   Configure font paths, PDF export options
    *   Settings for `tinymist` preview server configuration
*   **Full Server-Specific Interactions:**
    *   Systematically implement robust handlers for: `workspace/configuration` requests, `textDocument/didOpen|Change|Close` for auxiliary files, focus tracking notifications.
*   **Documentation:**
    *   Update plugin `README.md` (setup, features, settings).
    *   Keep `dev-notes.md` current.

### VI. Technical Debt & Refinements
*   **Missing File Type Icon**: TODO in `TypstLanguage.kt` - need to add custom icon for .typ files.
*   **LSP Initialization Options**: Currently commented out in `TinymistLspStreamConnectionProvider.kt` - initialization options for the LSP server (e.g., `colorTheme`, preview URL, `preview.background.enabled`) should be configurable via settings panel.

## Project Architecture and File Overview

This section outlines the architecture of the Tinymist IntelliJ plugin, detailing the roles of key files and their interactions, particularly with the IntelliJ Platform and LSP4IJ APIs.

### Core Directory Structure

*   **`editors/intellij/`**: Root directory for the IntelliJ plugin.
    *   **`build.gradle.kts`**: Gradle build script for managing dependencies (like `lsp4ij`, IntelliJ Platform SDK) and plugin packaging.
    *   **`src/main/kotlin/org/tinymist/intellij/`**: Contains the core Kotlin source code for the plugin. This is further structured into sub-packages like `lsp`, `preview`, and `structure`.
    *   **`src/main/resources/META-INF/plugin.xml`**: The plugin descriptor file, essential for IntelliJ to load and recognize the plugin and its components (e.g., language support, LSP integration, preview editors, structure view).

### Kotlin Source Files (`src/main/kotlin/org/tinymist/intellij/`)

The source code is organized into the following main areas:

1.  **Base Language Support (`org.tinymist.intellij`)**
    *   **`TypstLanguage.kt`**: Defines `TypstLanguage` (a subclass of `com.intellij.lang.Language`) and `TypstFileType` (a subclass of `com.intellij.openapi.fileTypes.LanguageFileType`). This is the fundamental registration of "Typst" as a recognized language and file type within the IntelliJ Platform.
    *   **`TypstFile.kt`**: Defines `TypstFile` (a subclass of `com.intellij.extapi.psi.PsiFileBase`). This class represents a Typst file in IntelliJ's Program Structure Interface (PSI) tree, allowing the platform to understand it as a structured file.
    *   **Local Parsing/Lexing/Highlighting**: The plugin **does not** currently include or register custom local lexers (`TypstLexerAdapter.kt`), parsers (`TypstParserDefinition.kt`), or syntax highlighters (`TypstSyntaxHighlighter.kt`). It relies on the LSP server for semantic tokens for syntax highlighting and for other structural understanding. The grammar files in `src/main/grammars/` are unused by the plugin's runtime.

2.  **LSP (Language Server Protocol) Integration (`org.tinymist.intellij.lsp`)**
    *   **`TinymistLanguageServerFactory.kt`**: Implements `com.redhat.devtools.lsp4ij.LanguageServerFactory`. Creates instances of `TinymistLspStreamConnectionProvider` for server connection, provides `TinymistLSPDiagnosticFeature` for custom diagnostic handling, and includes `TinymistLanguageServerInstaller` for automated server installation.
    *   **`TinymistLspStreamConnectionProvider.kt`**: Extends `com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider`. This class manages the lifecycle and communication with the `tinymist` LSP executable using sophisticated executable resolution:
        *   Uses `TinymistSettingsService` to determine server management mode (AUTO_MANAGE or CUSTOM_PATH)
        *   For AUTO_MANAGE mode: Uses `TinymistLanguageServerInstaller` to get automatically installed executable path
        *   For CUSTOM_PATH mode: Uses user-configured executable path from settings
        *   Initialization options are currently commented out (TODO) but previously provided server configuration
    *   **`TinymistLanguageServerInstaller.kt`**: Comprehensive auto-installation system that downloads and installs platform-specific tinymist binaries from GitHub releases. Supports Windows, macOS (x64/ARM64), and Linux (x64/ARM64) with proper archive extraction and executable permissions.
    *   **`TinymistLanguageClient.kt`**: Extends `com.redhat.devtools.lsp4ij.client.LanguageClientImpl`. This custom client handles Tinymist-specific LSP notifications and can customize how standard LSP messages are processed.
        *   **`@JsonNotification("tinymist/document") handleDocument(...)`**: Placeholder for handling a custom notification, potentially for preview updates or other document-specific events. (Currently logs receipt).
        *   **`publishDiagnostics(...)`**: Overrides the default handler to reformat diagnostic messages (errors, warnings) from the server (e.g., replacing newlines with `<br>`) for better display in IntelliJ's UI.
        *   **`showMessageRequest(...)`**: Overrides the default to handle `window/showMessageRequest` from the server, mainly to log them and prevent potential NPEs in `lsp4ij` if actions are null.

3.  **Settings Management (`org.tinymist.intellij.settings`)**
    *   **`TinymistSettingsService.kt`**: Application-level service that implements `PersistentStateComponent<TinymistSettingsState>` for persistent storage of plugin settings. Provides convenient accessors for `tinymistExecutablePath` and `serverManagementMode`.
    *   **`TinymistSettingsState.kt`**: Data class defining the plugin's settings state, including `ServerManagementMode` enum (AUTO_MANAGE vs CUSTOM_PATH) and executable path configuration.
    *   **`TinymistSettingsPanel.kt`**: Swing-based UI panel for the settings interface with radio buttons for server management mode and text field for custom executable path.
    *   **`TinymistSettingsConfigurable.kt`**: Implements `Configurable` interface to integrate the settings panel into IntelliJ's Settings/Preferences dialog under "Tools > Tinymist LSP".
    *   **`TinymistVersion.kt`**: Version management for the tinymist server, used by the installer to determine which version to download.

4.  **JCEF-based Preview (`org.tinymist.intellij.preview`)**
    *   **`TypstPreviewFileEditor.kt`**: Implements `com.intellij.openapi.fileEditor.FileEditor` and uses `com.intellij.ui.jcef.JCEFHtmlPanel` to embed a Chromium-based browser view. This editor displays the live preview of the Typst document.
        *   It connects to a web server (e.g., `http://127.0.0.1:23635`) that is started and managed by the `tinymist` language server itself (when `preview.background.enabled` is true).
        *   It includes logic to wait for the server to be available before attempting to load the URL.
        *   It handles cases where JCEF might not be supported in the user's environment.
    *   **`TypstPreviewFileEditorProvider.kt`**: Implements `com.intellij.openapi.fileEditor.FileEditorProvider`. This provider is responsible for creating instances of `TypstPreviewFileEditor` when IntelliJ needs to open a preview for a Typst file. It also defines the editor's ID and policy (e.g., where it should be placed relative to other editors).
    *   **`TypstTextEditorWithPreviewProvider.kt`**: Extends `com.intellij.openapi.fileEditor.TextEditorWithPreviewProvider`. This class is the main entry point registered in `plugin.xml` for opening Typst files. It combines a standard text editor (provided by IntelliJ) with the custom `TypstPreviewFileEditor` (obtained via `TypstPreviewFileEditorProvider`), allowing for a side-by-side text and preview editing experience. It accepts files of type `TypstFileType`.

### Key Interactions

*   **IntelliJ Platform & Plugin Startup**: IntelliJ reads `plugin.xml` to discover the plugin's capabilities. It registers `TypstLanguage` and `TypstFileType`.
*   **Opening a Typst File**:
    *   `TypstTextEditorWithPreviewProvider` is invoked, creating a split editor with a text part and a `TypstPreviewFileEditor`.
    *   `TinymistLanguageServerFactory` is triggered, which starts `TinymistLspStreamConnectionProvider` to launch the `tinymist` LSP server process.
    *   `TinymistLanguageClient` establishes communication with the server.
*   **LSP Communication**:
    *   The client and server exchange JSON-RPC messages for features like diagnostics, completion, hover, etc.
    *   `TinymistLanguageClient` handles custom notifications like `tinymist/document` (currently a placeholder). A `tinymist/documentOutline` handler would be needed for a Structure View.
*   **Structure View**:
    *   (Currently not implemented as described in `dev-notes.md`). If implemented, when the user opens the Structure View, a `TypstStructureViewFactory` would create a `TypstStructureViewModel`.
    *   The view model would fetch data (potentially from an `OutlineDataHolder` populated by `TinymistLanguageClient`) and build the tree.
*   **Preview Panel**:
    *   `TypstPreviewFileEditor` loads its content from the HTTP server run by the `tinymist` LSP (if `preview.background.enabled` is true in initialization options).
    *   Updates to the preview are likely driven by the `tinymist` server itself, potentially triggered by `textDocument/didChange` notifications from the client or its own file watching.

This architecture aims to delegate most of the complex language understanding and preview rendering to the external `tinymist` LSP server, while the IntelliJ plugin focuses on integrating these features into the IDE's UI and user experience, adhering to IntelliJ Platform and `lsp4ij` conventions.