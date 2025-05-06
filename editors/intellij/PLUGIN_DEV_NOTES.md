# Tinymist IntelliJ Plugin Development Notes

## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs. This is achieved by integrating the `tinymist` language server ([https://github.com/Myzel394/tinymist](https://github.com/Myzel394/tinymist)) into the IntelliJ Platform using the `lsp4ij` plugin developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)). The plugin aims to offer features such as syntax highlighting, autocompletion, diagnostics, hover information, go-to-definition, and potentially more, mirroring the capabilities of the Tinymist VSCode extension.


## Current Next Steps

The immediate priority is to resolve the language server startup failure, which currently prevents any LSP features from functioning. The primary goal after resolving this is to achieve basic linting (diagnostics).

**Phase 1: Resolve Server Startup Crash**
*   **Objective:** Ensure the `tinymist` language server can be successfully registered and started by `lsp4ij` without the `IllegalArgumentException` ("mappings must not be null" error).
*   **Key Actions:**
    *   **Verify Build Environment & Caches:**
        *   Perform a full Gradle clean (`./gradlew clean`).
        *   In the IntelliJ IDEA development instance, invalidate all caches ("File" > "Invalidate Caches..." > select all options > "Invalidate and Restart").
        *   Rebuild the plugin project.
        *   Retest running the plugin (`./gradlew runIde`).
    *   **Analyze `idea.log`:** If the crash persists after the above, meticulously examine the `idea.log` from the *target* IntelliJ instance (launched by `runIde`). Look for any errors or warnings related to `org.tinymist.intellij` components (especially `TypstFileType`, `TypstLanguage`) or `lsp4ij` that occur *before* the main `IllegalArgumentException`. These could indicate the root cause (e.g., class loading issues, problems with `TypstLanguage.kt` or `plugin.xml` interactions).
    *   **Confirm Core Language Definitions:** Re-verify `TypstLanguage.kt` (containing `TypstLanguage` and `TypstFileType` objects) and its registration in `plugin.xml` to ensure the "Typst" language ID is correctly and unambiguously defined and available to the platform before `lsp4ij` initialization. (Recent simplifications to `TypstLanguage.kt` were a diagnostic step for this; if the crash is resolved, the necessity of the removed `IElementType` should be re-evaluated).

**Phase 2: Achieve Basic Linting (Diagnostics)**
*   **Objective:** Verify that diagnostics (linting errors/warnings) from the `tinymist` server are correctly displayed in the IntelliJ editor for `.typ` files.
*   **Prerequisites:** Phase 1 (Server Startup Crash) must be resolved.
*   **Key Actions:**
    *   **Confirm Server Emits Diagnostics:** Ensure the `tinymist lsp` server itself is configured and capable of generating and sending `textDocument/publishDiagnostics` notifications for Typst files. This may involve testing the `tinymist` LSP separately or reviewing its capabilities.
    *   **Verify `lsp4ij` Processes Diagnostics:** Once the server is running and theoretically sending diagnostics, open a `.typ` file with known errors. Confirm that `lsp4ij` receives these diagnostics and correctly translates them into IntelliJ editor annotations (e.g., squiggly underlines, entries in the "Problems" view).
    *   **Basic `IElementType` (If Necessary):** If the `TYPST_TEXT` `IElementType` (previously in `TypstLanguage.kt`) was removed as a diagnostic and its absence prevents basic lexing/parsing required by the IntelliJ platform (even before full LSP features), it might need to be carefully reintroduced or an alternative minimal lexer/parser setup considered. This is to ensure the file is recognized sufficiently by IntelliJ for `lsp4ij` to operate on it.

**Phase 3: Implement Core LSP Features & Settings (Revisit Original Next Steps)**
*   Once the server starts reliably and basic diagnostics are working, proceed with the broader feature implementation plan:
    1.  **Implement IntelliJ Settings Panel:**
        *   Create a dedicated settings/preferences page for Tinymist (e.g., under "Languages & Frameworks" or "Tools").
        *   Allow configuration of: Path to the `tinymist` executable, font paths (if needed by the server), PDF export settings, preview-related settings, and other relevant options from the VSCode `TinymistConfig`.
    2.  **Load Settings into `TinymistInitializationOptions`:**
        *   In `TinymistLspStreamConnectionProvider#getInitializationOptions`, retrieve configured values from the settings panel and correctly populate the `TinymistInitializationOptions` data class.
    3.  **Enhance `findTinymistExecutable()` in `TinymistLspStreamConnectionProvider`:**
        *   Prioritize the path configured in IntelliJ settings.
        *   Fall back to searching `PATH`.
        *   Consider options for bundling `tinymist` or providing clear download/setup instructions.
    4.  **User-Friendly Notifications & Error Handling:**
        *   If the `tinymist` executable is not found/started, display a clear notification guiding the user to settings.
        *   Leverage `lsp4ij`'s error reporting for server-side issues.
    5.  **Address Server-Specific Interactions (from "Insights" section):**
        *   Systematically investigate and implement handlers for: `workspace/configuration` requests, sending `textDocument/didOpen|Change|Close` for auxiliary files (if needed by `tinymist`), focus tracking notifications (if needed).
        *   Continue to monitor and address any file watcher issues if they arise.
    6.  **Testing and Refinement of Core LSP Features:**
        *   Thoroughly test: completions, hover information, go-to-definition, semantic highlighting (if provided by the server).
        *   Test with various Typst projects and edge cases.
        *   Profile performance and optimize if necessary.
    7.  **Preview Panel Integration (Longer Term):**
        *   Plan and implement an integrated preview panel for Typst documents.
    8.  **Documentation:**
        *   Update the plugin's `README.md` with setup instructions and feature overview.
        *   Ensure these development notes (`PLUGIN_DEV_NOTES.md`) are kept up-to-date with progress and any new findings.
