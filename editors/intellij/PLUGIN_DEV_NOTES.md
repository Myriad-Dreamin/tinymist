# Tinymist IntelliJ Plugin Development Notes

## Project Scope

The goal of this project is to provide comprehensive Typst language support for IntelliJ-based IDEs. This is achieved by integrating the `tinymist` language server ([https://github.com/Myzel394/tinymist](https://github.com/Myzel394/tinymist)) into the IntelliJ Platform using the `lsp4ij` plugin developed by Red Hat ([https://github.com/redhat-developer/lsp4ij](https://github.com/redhat-developer/lsp4ij)). The plugin aims to offer features such as syntax highlighting, autocompletion, diagnostics, hover information, go-to-definition, and potentially more, mirroring the capabilities of the Tinymist VSCode extension.


## Current Status and Next Steps

**Status as of 2025-05-06:**
*   **Phase 1: Resolve Server Startup Crash - COMPLETED**
    *   The language server (`tinymist`) now starts successfully. The `IllegalArgumentException` ("mappings must not be null" error) was resolved by:
        1.  Adding the `TYPST_TEXT` `IElementType` definition.
        2.  Switching from direct `ProcessStreamConnectionProvider` implementation in `plugin.xml` to using a `LanguageServerFactory` (`TinymistLanguageServerFactory`).
        3.  Ensuring the `TinymistLanguageServerFactory` and `TinymistLspStreamConnectionProvider` constructors correctly handled non-nullable `Project` parameters.
        4.  Changing the LSP mapping in `plugin.xml` from `<languageMapping>` to `<fileNamePatternMapping patterns="*.typ" languageId="typst"/>`.
*   **Phase 2: Achieve Basic Linting (Diagnostics) - COMPLETED**
    *   Basic diagnostics (linting errors/warnings) from the `tinymist` server are now correctly displayed in the IntelliJ editor for `.typ` files, as observed after resolving the startup crash.

**Phase 3: Enable and Test Core LSP Features**
*   **Objective:** Ensure that core LSP features beyond diagnostics are functioning correctly. This includes, but is not limited to:
    *   Completions
    *   Hover Information
    *   Go-to-Definition
    *   Semantic Highlighting (if provided by the server and distinct from basic lexer-based highlighting).
*   **Key Actions:**
    1.  **Systematic Testing:** Test each of the above features with various Typst projects and edge cases. Document findings and any issues encountered.
    2.  **Review `TinymistInitializationOptions.kt`:**
        *   Examine the current `lspInputs: {"x-preview": "{\"version\":1,\"theme\":\"\"}"}`. Determine its exact purpose for `tinymist` and if it's correctly configured for core features or if it should be modified/removed for this phase.
        *   Identify other `initializationOptions` that `tinymist` might expect or require for these core LSP features to function optimally. This may involve looking at the Tinymist VSCode extension's configuration or `tinymist` server documentation if available.
        *   Update `TinymistInitializationOptions.kt` and its population in `TinymistLspStreamConnectionProvider#getInitializationOptions` as needed.
    3.  **Address Server-Specific Interactions (from "Insights from VSCode Extension..." section - if blocking core features):**
        *   Based on testing, investigate if any of the following are immediately necessary for the core features listed above to work correctly. Implement minimal, targeted handlers if they are identified as blockers:
            *   Handling `workspace/configuration` requests from the server.
            *   Sending `textDocument/didOpen|Change|Close` for auxiliary files (e.g., `.bib`, images) if `tinymist` relies on this for context for core features.
            *   Focus tracking notifications (e.g., `tinymist/setActiveTextEditor`) if lack of this breaks context-sensitive operations like completions or go-to-definition.
        *   Defer full, robust implementation of these (and Preview Management) until Phase 4, unless they prove essential for basic feature operability.
    4.  **File Watcher Issues:** Continue to monitor for any file watcher issues as mentioned in the original "Insights" section, as these could affect diagnostics or other features.
    5.  **Profile performance and optimize if necessary (ongoing).**

**Phase 4: Implement Settings, Improve User Experience, and Advanced Features (Previously part of Phase 3)**
*   Once core LSP features are verified and stable:
    1.  **Implement IntelliJ Settings Panel:**
        *   Create a dedicated settings/preferences page for Tinymist (e.g., under "Languages & Frameworks" or "Tools").
        *   Allow configuration of: Path to the `tinymist` executable, font paths, PDF export settings, preview-related settings, and other relevant options derived from `TinymistConfig` (VSCode) and `TinymistInitializationOptions`.
    2.  **Load Settings into `TinymistInitializationOptions`:**
        *   In `TinymistLspStreamConnectionProvider#getInitializationOptions`, retrieve configured values from the settings panel and correctly populate the `TinymistInitializationOptions` data class.
    3.  **Enhance `findTinymistExecutable()` in `TinymistLspStreamConnectionProvider` & Error Handling:**
        *   Modify the `init` block of `TinymistLspStreamConnectionProvider` (and `findTinymistExecutable`) to:
        *   Prioritize the path configured in IntelliJ settings.
        *   Fall back to searching `PATH`.
            *   If the executable is not found or invalid, display a user-friendly IntelliJ notification (e.g., a balloon notification with a link to settings) instead of throwing a `RuntimeException`. Prevent LSP connection attempts if the path is invalid.
        *   Consider options for bundling `tinymist` or providing clear download/setup instructions within the settings UI.
    4.  **Full Implementation of Server-Specific Interactions:**
        *   Systematically implement robust handlers for: `workspace/configuration` requests, sending `textDocument/didOpen|Change|Close` for auxiliary files, and focus tracking notifications, based on a deeper understanding of `tinymist`'s requirements.
    5.  **Preview Panel Integration (Longer Term):**
        *   Plan and implement an integrated preview panel for Typst documents, including handling custom messages like `tinymist/previewStart`, `tinymist/updatePreview`, etc.
    6.  **Documentation:**
        *   Update the plugin's `README.md` with setup instructions, feature overview, and settings guide.
        *   Ensure these development notes (`PLUGIN_DEV_NOTES.md`) are kept up-to-date.
