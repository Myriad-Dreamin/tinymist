# Tinymist IntelliJ Plugin Development Notes
> WARNING: AI Code Slop ahead


## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs.
We are using the `lsp4ij` library developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)).

## Development Instructions

1.  **Prerequisites:**
    *   **IntelliJ IDEA:** For developing IntelliJ plugins, this is the most convenient IDE.


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
    *   The plugin currently relies on `TinymistLspStreamConnectionProvider.kt` to find the `tinymist` executable. It searches the system `PATH`.
    *   Ensure that `tinymist` is installed on your system.

7.  **Viewing Logs:**
    *   **IntelliJ Plugin Logs:** Check the `idea.log` file of the sandboxed IntelliJ instance. You can find its location via "Help" > "Show Log in Finder/Explorer" in the sandbox IDE.
    *   **LSP Communication Logs:** `lsp4ij` provides an "LSP Consoles" view in the sandbox IDE (usually accessible from the tool window bar at the bottom left). Set its verbosity (e.g., to "verbose") via `Languages & Frameworks > Language Servers` settings to see JSON-RPC messages between the plugin and `tinymist`.


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
    *   **Issue:** Significant scrolling lag/input delay in the JCEF-based preview panel. Lag is affected by JCEF DevTools/FPS meter. https://github.com/Myriad-Dreamin/tinymist/issues/1746 
    *   **Frontend Build Workflow Confirmed for printf debugging**
        1.  Build frontend: `yarn build:preview` (from `tinymist` root) -> copies `typst-preview.html` to `crates/tinymist-assets/src/`.
        2.  Configure `tinymist/Cargo.toml`: Use local path for `tinymist-assets` (`tinymist-assets = { path = "./crates/tinymist-assets/" }`).
        3.  Rebuild `tinymist`: `cargo build`.
    *   **Detailed Frontend Logging for Input Lag Investigation:**
        *   **Initial Problem:** Significant scrolling input lag and delayed visual updates in the JCEF-based Typst preview panel. The behavior is notably influenced by whether the JCEF DevTools (especially the FPS meter) are open, suggesting a performance bottleneck or an issue related to how the browser component handles rendering or event processing under certain conditions.
        *   **Objective:** To pinpoint the cause of the input lag by instrumenting the frontend JavaScript with detailed performance and event logging. The goal is to understand if the JavaScript main thread is stalling, where time is being spent during critical operations (scrolling, resizing, WebSocket message processing), and how these timings correlate with the observed lag and the state of JCEF DevTools.
        *   **Method:** Added extensive `console.log` and `console.time/timeEnd` statements to the frontend JavaScript (`tools/typst-preview-frontend/src/main.js` and `tools/typst-preview-frontend/src/ws.ts`).
        *   **Hypotheses Under Investigation:**
            1.  **Main Thread Stalls:** The JavaScript main thread is blocked by long-running synchronous operations during scroll or render updates, causing delayed event processing and visual updates. (Test via `requestAnimationFrame` delta logging).
            2.  **Inefficient Scroll Event Handling:** The current `debounceTime` (or lack of `throttle`) in scroll event handling (`main.js` or `ws.ts`) is causing poor perceived performance or triggering updates inefficiently. (Tested by switching to `throttle`).
            3.  **Costly `svgDoc.addViewportChange()`:** The function called by the scroll handler to notify the rendering engine about the viewport change is computationally expensive. (Test via `console.time/timeEnd`).
            4.  **Costly Rendering Updates (`svgDoc.addChangement()`):** The actual application of document changes (diffs) by the rendering engine is too slow, especially for large or frequent updates. (Test via `console.time/timeEnd` around `addChangement` in `ws.ts`).
            5.  **WebSocket Message Batching/Processing:** The way WebSocket messages are batched or processed sequentially introduces delays. (Test by examining `rxjs` `buffer` and `debounceTime(0)` behavior and `processMessage` loop).
            6.  **JCEF/Browser Bottleneck:** The interaction between JS, rendering, and the JCEF environment itself creates a bottleneck, especially when DevTools are closed. (Indirectly tested by observing behavior with/without DevTools).
            7.  **Selection-Related Performance Issue:** Changes to text selection might trigger expensive updates or computations, contributing to the lag (Hypothesis from GitHub issue comment).

        *   **Log Analysis (15 May - After extensive logging additions):**
            *   **Scroll Event Handling (`main.js`):**
                *   Raw scroll events are frequent.
                *   Debounced scroll handler (`svgDoc.addViewportChange`) execution is very fast (~0.1ms).
                *   `requestAnimationFrame` heartbeat delta does not show significant JS main thread stalls during typical interaction.
            *   **WebSocket Message Processing (`ws.ts`):**
                *   `processMessage` (excluding `addChangement`) is generally fast.
                *   `svgDoc.addChangement` (called within `processMessage` for `diff-v1` updates) shows variable execution time, ranging from ~1ms to ~15ms in observed logs. This is a key area of interest.
            *   **WebSocket Stability & File Watcher:**
                *   Frequent WebSocket disconnects (code `1006`) were observed.
                *   These disconnects correlate strongly with `tinymist` backend errors: `NotifyActor: failed to get event, exiting...` (a file watcher issue).
                *   This file watcher error seems to occur primarily during IDE/project shutdown or sometimes during initial project load, not consistently during active scrolling lag. It's likely a separate issue, possibly related to macOS file descriptor limits or `notify-rs` crate behavior.
                *   Related GitHub issues for `tinymist` (#1614, #1534, etc.) point to this being a known problem on macOS, sometimes resolved by `notify-rs` updates.
            *   **JCEF DevTools Impact:** The observation that lag significantly reduces or disappears when JCEF DevTools are open remains a strong indicator that the bottleneck is heavily influenced by the browser's rendering pipeline or event loop timing, which DevTools can alter.

        *   **Current Status & Next Steps (Input Lag Investigation):**
            *   **Linter Errors in `ws.ts`:** The most recent logging additions to `ws.ts` (for detailed `addChangement` timing) have introduced TypeScript linter errors that need to be resolved:
                *   `All declarations of 'typstWebsocket' must have identical modifiers.`
                *   `Subsequent property declarations must have the same type.  Property 'typstWebsocket' must be of type 'WebSocket', but here has type 'WebSocket | undefined'.`
                *   `Type 'string | Uint8Array<ArrayBuffer>' is not assignable to type 'string'.` (for `svgDoc.addChangement([message[0], message[1]])`)
            *   **Refined Hypothesis:** The input lag is likely not due to a single long-blocking JS function, but rather:
                1.  The cumulative effect of frequent, moderately expensive rendering updates (`addChangement`) triggered by WebSocket messages.
                2.  A bottleneck within the JCEF rendering/compositing process, exacerbated when DevTools are closed.
            *   **Immediate Next Step:** Resolve the linter errors in `tools/typst-preview-frontend/src/ws.ts` to ensure accurate logging and type safety.
            *   **Following Steps:**
                1.  Re-run with corrected logging and capture logs specifically during scroll lag.
                2.  Focus analysis on the frequency and duration of `addChangement` calls in relation to scroll events and perceived lag.
                3.  Consider experiments to reduce `addChangement` frequency or payload if it's identified as the primary contributor.
                4.  Continue to treat the WebSocket `1006` / file watcher error as a separate stability issue, though it might indirectly affect overall performance if it causes frequent preview reloads.
        *   **Debugging Session (LATEST - 2024-05-16): Investigating `window.svgDoc` and Build Issues**
            *   **Objective:** Ensure `window.svgDoc` is correctly initialized and accessible in `main.js` to allow scroll event handling, and resolve any build issues preventing this.
            *   **Key Findings & Actions:**
                *   **JCEF Logging Confirmed:** Successfully configured JCEF to output JavaScript `console.log` messages to `editors/intellij/logs.log`.
                *   **`window.svgDoc` Initialization:**
                    *   Identified that `svgDoc` (an instance of `TypstDocument`) was created in `tools/typst-preview-frontend/src/ws.ts` but not assigned to `window.svgDoc`.
                    *   Added `window.svgDoc = svgDoc;` in `ws.ts` within the `plugin.runWithSession` callback.
                    *   Updated the `declare global { interface Window { ... } }` block in `ws.ts` to include `svgDoc?: TypstDocument;`.
                *   **Build Errors & Fixes (TypeScript):** Addressed several TypeScript errors in `ws.ts`.
                *   **Build Successful:** After these changes, `yarn build:preview; cargo build` completed successfully.
            *   **Current Status & Deeper Dive into Gray Screen (Update from current debugging session):**
                *   The build remains successful, and the `window.svgDoc` assignment logic is in place.
                *   **Persistent Issue (Gray Screen):** The JCEF preview panel consistently renders as a gray screen. This is the primary blocker.
                    *   (Note: The original input lag issue is confirmed to be JCEF-specific, as the preview URL in a standalone regular browser does not exhibit the same lag. However, the gray screen prevents further lag analysis in JCEF.)
                *   **Gray Screen Investigation So Far:**
                    *   Initial JavaScript execution in `ws.ts` (`wsMain`) *is* occurring. Test code successfully found the `#typst-app` div and programmatically set its `innerHTML` to a test `<h1>` tag.
                    *   Despite this JavaScript modification, the JCEF panel remains visually gray.
                    *   DOM inspection using JCEF DevTools revealed that the `#typst-app` div was subsequently empty or reported 0x0 dimensions after `wsMain` proceeded through `createSvgDocumentAndSetup`.
                    *   This implies:
                        1.  The injected test `<h1>` is being cleared (most likely by the `hookedElem.innerHTML = "";` line within `createSvgDocumentAndSetup`).
                        2.  Subsequently, `TypstDocument` (initialized in `createSvgDocumentAndSetup`) fails to render any visible content into `#typst-app`, or fails to ensure `#typst-app` receives non-zero dimensions.
                    *   The `"Uncaught Error: Attempt to use a moved value"` (previously triggered when `svgDoc.reset()` was called on WebSocket open) is likely a symptom of an earlier initialization fault rather than the root cause of the gray screen, as the gray screen persists even when `svgDoc.reset()` is bypassed.
                *   **Latest Action (End of This Session):** Added detailed logging in `tools/typst-preview-frontend/src/ws.ts` (within the `plugin.runWithSession` callback). This logging captures the `innerHTML` and `clientWidth`/`clientHeight` of the `#typst-app` div immediately *before* and *after* the `createSvgDocumentAndSetup(kModule)` call.
                *   **Next Step (Next Session):**
                    1.  Run the application with the latest logging additions.
                    2.  Analyze the JCEF DevTools console output to observe the logged states of `#typst-app`:
                        *   Confirm if the test `<h1>` (injected by `wsMain`) is present in `#typst-app`'s `innerHTML` *before* `createSvgDocumentAndSetup` is called.
                        *   Examine the `innerHTML` and dimensions of `#typst-app` *after* `createSvgDocumentAndSetup` has executed. (Is it empty? Does it contain an `<svg>` element? What are its dimensions reported by `clientWidth`/`clientHeight`?).
                    3.  Based on these logs, the goal is to determine more precisely whether `TypstDocument` fails to populate `#typst-app` after it's cleared, or if the content it adds is simply not visible/sized correctly.

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

### IV. Next Steps
*   **`textDocument/references` (Find Usages):** Implement this core LSP feature.
*   **Stabilize Preview Panel Integration:**
    *   Based on feedback from the GitHub issue and potential fixes, ensure smooth and reliable preview rendering and interaction.
    *   Refine LSP interaction for preview if needed (e.g., scroll sync, theme changes via `JBCefJSQuery`).

### V. Planned Features & Enhancements
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
*   **Minimal Client-Side Parsing/Lexing**: These are currently implemented as minimal boilerplate (see `TypstLexerAdapter.kt`, `TypstParserDefinition.kt`) required by the IntelliJ Platform for basic file type recognition and PSI structure. The LSP handles rich parsing and structural understanding.
*   **Basic Client-Side Syntax Highlighter**: Similarly, `TypstSyntaxHighlighter.kt` provides a very basic highlighter as IntelliJ Platform boilerplate. Rich syntax highlighting is provided by the LSP via semantic tokens.
*   **LSP `tinymist/documentOutline` URI Issue & Mock Data:**
    *   **Problem:** The `tinymist` LSP server currently does not consistently provide a usable `uri` in its `tinymist/documentOutline` notifications. This prevents the client from associating outline data with specific files.
    *   **Current State:** `TinymistLanguageClient.kt` logs a warning and `OutlineDataHolder` falls back to providing mock data for the Structure View.
    *   **Resolution:** Requires a server-side fix in `tinymist` to ensure a valid `file://` URI is always sent. A `TODO` tracks this in the client code.
*   **`lsp4ij` `showMessageRequest` NullPointerException Workaround:**
    *   **Problem:** The `lsp4ij` library can throw a `NullPointerException` if the LSP server sends a `window/showMessageRequest` with a `null` actions list.
    *   **Workaround:** `TinymistLanguageClient.kt` overrides `showMessageRequest` to log the request and return a `CompletableFuture.completedFuture(null)`, bypassing the problematic `lsp4ij` handler and suppressing the UI for these messages.
*   Rudimentary LSP Executable Error Handling (Partially addressed by "Robust `tinymist` Executable Handling" above).
*   Missing File Type Icon.
*   Hardcoded Configuration Defaults in `TinymistInitializationOptions` (e.g., `colorTheme`, preview URL - review what should be settings).
*   Incomplete Parser Definition Features (Evaluate if still relevant).
*   JCEF Preview Placeholder Content: Largely addressed as `tinymist` serves its own UI.

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
    *   **`TypstLexerAdapter.kt`**: Implements `com.intellij.lexer.Lexer`. It provides a very basic lexer that currently treats the entire file content as a single generic token (`TYPST_TEXT`). This is a minimal implementation, as detailed lexing for features like advanced syntax highlighting or code analysis is primarily handled by the `tinymist` LSP server.
    *   **`TypstParserDefinition.kt`**: Implements `com.intellij.lang.ParserDefinition`.
        *   It supplies the `TypstLexerAdapter`.
        *   It provides a basic `PsiParser` that creates a single root PSI node for the file. Similar to the lexer, this is kept minimal because the LSP server is responsible for the in-depth understanding of the code's structure.
        *   It specifies how to create `TypstFile` PSI elements.
        *   It defines `TYPST_TEXT` as an `IElementType` (a type for tokens).
    *   **`TypstSyntaxHighlighter.kt` (contains `TypstSyntaxHighlighter` and `TypstSyntaxHighlighterFactory`)**:
        *   `TypstSyntaxHighlighterFactory` implements `com.intellij.openapi.fileTypes.SyntaxHighlighterFactory` and is responsible for providing instances of `TypstSyntaxHighlighter`.
        *   `TypstSyntaxHighlighter` (a subclass of `com.intellij.openapi.fileTypes.SyntaxHighlighterBase`) uses the `TypstLexerAdapter`. It assigns a default text attribute (e.g., standard identifier color) to the `TYPST_TEXT` token. Rich, context-aware syntax highlighting is provided by the `tinymist` LSP server through semantic token support.

2.  **LSP (Language Server Protocol) Integration (`org.tinymist.intellij.lsp`)**
    *   **`TinymistLanguageServerFactory.kt`**: Implements `com.redhat.devtools.lsp4ij.LanguageServerFactory`. Its main role is to create and provide instances of `TinymistLspStreamConnectionProvider` (to connect to the server) and `TinymistLanguageClient` (to handle communication). This factory is registered in `plugin.xml` to enable LSP support for the Typst language.
    *   **`TinymistLspStreamConnectionProvider.kt`**: Extends `com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider`. This class is critical for managing the lifecycle and communication with the `tinymist` LSP executable.
        *   It locates the `tinymist` executable (currently hardcoded, but ideally from settings or PATH).
        *   It configures the command to start the LSP server (e.g., `["/path/to/tinymist", "lsp"]`).
        *   It provides initialization options (`getInitializationOptions()`) to the `tinymist` server when it starts. These options are sent as part of the LSP `initialize` request and can configure server behavior (e.g., enabling background preview).
    *   **`TinymistLanguageClient.kt`**: Extends `com.redhat.devtools.lsp4ij.client.LanguageClientImpl`. This custom client handles Tinymist-specific LSP notifications and can customize how standard LSP messages are processed.
        *   **`@JsonNotification("tinymist/documentOutline") onDocumentOutline(...)`**: Handles a custom notification from the `tinymist` server that provides document structure information (headings, labels, etc.). It parses this data and updates the `OutlineDataHolder` for use in the Structure View.
        *   **`@JsonNotification("tinymist/document") handleDocument(...)`**: Placeholder for handling another custom notification, potentially for preview updates or other document-specific events.
        *   **`publishDiagnostics(...)`**: Overrides the default handler to reformat diagnostic messages (errors, warnings) from the server (e.g., replacing newlines with `<br>`) for better display in IntelliJ's UI.
        *   **`showMessageRequest(...)`**: Overrides the default to handle `window/showMessageRequest` from the server, mainly to log them and prevent potential NPEs in `lsp4ij` if actions are null.
    *   **`TinymistOutlineModel.kt`**: Defines Kotlin data classes (`TinymistDocumentOutlineParams`, `TinymistOutlineItem`) used to deserialize the JSON payload of the `tinymist/documentOutline` notification. These classes model the structure of an outline item (name, kind, detail, range, children).
    *   **`TypstFindUsagesProvider.kt`**: Implements `com.intellij.lang.findUsages.FindUsagesProvider`. Registered in `plugin.xml`, it integrates with IntelliJ's "Find Usages" feature. It relies on the LSP server (via `lsp4ij`) to perform the actual search for references.

3.  **JCEF-based Preview (`org.tinymist.intellij.preview`)**
    *   **`TypstPreviewFileEditor.kt`**: Implements `com.intellij.openapi.fileEditor.FileEditor` and uses `com.intellij.ui.jcef.JCEFHtmlPanel` to embed a Chromium-based browser view. This editor displays the live preview of the Typst document.
        *   It connects to a web server (e.g., `http://127.0.0.1:23635`) that is started and managed by the `tinymist` language server itself (when `preview.background.enabled` is true).
        *   It includes logic to wait for the server to be available before attempting to load the URL.
        *   It handles cases where JCEF might not be supported in the user's environment.
    *   **`TypstPreviewFileEditorProvider.kt`**: Implements `com.intellij.openapi.fileEditor.FileEditorProvider`. This provider is responsible for creating instances of `TypstPreviewFileEditor` when IntelliJ needs to open a preview for a Typst file. It also defines the editor's ID and policy (e.g., where it should be placed relative to other editors).
    *   **`TypstTextEditorWithPreviewProvider.kt`**: Extends `com.intellij.openapi.fileEditor.TextEditorWithPreviewProvider`. This class is the main entry point registered in `plugin.xml` for opening Typst files. It combines a standard text editor (provided by IntelliJ) with the custom `TypstPreviewFileEditor` (obtained via `TypstPreviewFileEditorProvider`), allowing for a side-by-side text and preview editing experience. It accepts files of type `TypstFileType`.

4.  **Structure View Integration (`org.tinymist.intellij.structure`)**
    *   **`TypstStructureViewFactory.kt`**: Implements `com.intellij.lang.PsiStructureViewFactory`. This factory is registered in `plugin.xml` for the `TypstLanguage`. When the user opens the Structure View for a Typst file, IntelliJ calls this factory to get a `StructureViewBuilder`.
    *   **`TypstStructureViewModel.kt`**: Extends `com.intellij.ide.structureView.StructureViewModelBase`. It provides the data model for the Structure View tree.
        *   It takes a `PsiFile` (the Typst file being viewed) and an optional `Editor`.
        *   The root of the structure view is represented by `TypstStructureViewRootElement` (an inner class).
        *   It populates the structure view by retrieving outline items from `OutlineDataHolder.getOutline(filePath)` and wrapping them in `TypstStructureViewElement` instances.
    *   **`TypstStructureViewElement.kt`**: Implements `com.intellij.ide.structureView.StructureViewTreeElement`, `com.intellij.ide.util.treeView.smartTree.SortableTreeElement`, and `com.intellij.navigation.NavigationItem`. Each instance of this class represents a node in the Structure View (e.g., a heading).
        *   It wraps a `TinymistOutlineItem` (from the LSP server).
        *   It provides presentation details (text, icon, location string).
        *   It handles navigation: when a user clicks an item in the Structure View, the `navigate()` method uses an `OpenFileDescriptor` to move the cursor to the corresponding location in the editor, converting LSP `Range` objects to IntelliJ `TextRange` objects.
        *   It defines children recursively, building the tree structure.
    *   **`OutlineDataHolder.kt`**: A singleton object (`object`) that acts as a cache for document outline data.
        *   The `TinymistLanguageClient` calls `OutlineDataHolder.updateOutline(filePath, items)` when it receives new outline information from the LSP server via the `tinymist/documentOutline` notification.
        *   `TypstStructureViewModel` calls `OutlineDataHolder.getOutline(filePath)` to retrieve the cached (or mock, if unavailable) outline items for a given file.
        *   It includes logic to provide mock outline data if real data for a file path hasn't been received, which is useful for development or if the LSP server fails to send the outline.

### Key Interactions

*   **IntelliJ Platform & Plugin Startup**: IntelliJ reads `plugin.xml` to discover the plugin's capabilities. It registers `TypstLanguage` and `TypstFileType`.
*   **Opening a Typst File**:
    *   `TypstTextEditorWithPreviewProvider` is invoked, creating a split editor with a text part and a `TypstPreviewFileEditor`.
    *   `TinymistLanguageServerFactory` is triggered, which starts `TinymistLspStreamConnectionProvider` to launch the `tinymist` LSP server process.
    *   `TinymistLanguageClient` establishes communication with the server.
*   **LSP Communication**:
    *   The client and server exchange JSON-RPC messages for features like diagnostics, completion, hover, document outline, etc.
    *   `TinymistLanguageClient` handles custom notifications like `tinymist/documentOutline`.
*   **Structure View**:
    *   When the user opens the Structure View, `TypstStructureViewFactory` creates `TypstStructureViewModel`.
    *   The view model fetches data from `OutlineDataHolder` (populated by `TinymistLanguageClient`) and builds the tree using `TypstStructureViewElement` nodes.
*   **Preview Panel**:
    *   `TypstPreviewFileEditor` loads its content from the HTTP server run by the `tinymist` LSP (if `preview.background.enabled` is true in initialization options).
    *   Updates to the preview are likely driven by the `tinymist` server itself, potentially triggered by `textDocument/didChange` notifications from the client or its own file watching.

This architecture aims to delegate most of the complex language understanding and preview rendering to the external `tinymist` LSP server, while the IntelliJ plugin focuses on integrating these features into the IDE's UI and user experience, adhering to IntelliJ Platform and `lsp4ij` conventions.