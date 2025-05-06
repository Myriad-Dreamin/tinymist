package org.tinymist.intellij.lsp

// Sub-data classes for structuring initialization options, based on VSCode config and server logs

data class CompileFontOptions(
    val fontPaths: List<String> = emptyList(),
    val ignoreSystemFonts: Boolean = false
)

data class CompletionOptions(
    // Assuming a general enable flag for now. Specific sub-options can be added if needed.
    val enabled: Boolean = true
)

data class LintOptions(
    // Diagnostics are working, implying linting is enabled by default or via current settings.
    val enabled: Boolean = true
)

/**
 * Defines the initialization options sent to the Tinymist language server.
 * This structure should aim to mirror the configuration options the server expects,
 * similar to what the VSCode extension provides from its `TinymistConfig`.
 */
data class TinymistInitializationOptions(
    // `serverPath` was removed as it's primarily for client-side server discovery,
    // not usually an option the server itself consumes directly in `initializationOptions`.

    // Retained from previous version, possibly related to preview or other functionalities.
    // Defaulting to an empty map might be safer if its specific role isn't clear yet.
    val lspInputs: Map<String, String> = mapOf("x-preview" to "{\"version\":1,\"theme\":\"\"}"),

    // Defaulting to "light" theme, similar to VSCode's initial approach.
    val colorTheme: String = "light",

    // Font configuration options.
    val font: CompileFontOptions = CompileFontOptions(),

    // Changed to String based on server warning: "unknown variant `enabled`, expected `disable` or `enable`"
    // The server log for its internal config shows `semantic_tokens: Enable` (likely an enum variant).
    // Sending "enable" as a string is a common way to represent this in JSON for such enums.
    val semanticTokens: String = "enable",

    val completion: CompletionOptions = CompletionOptions(),
    val lint: LintOptions = LintOptions()

    // Placeholder for other potential root-level options from TinymistConfig:
    // val typingContinueCommentsOnNewline: Boolean? = null,
    // val exportPdf: String? = null, // e.g., "onSave", "never"
    // val outputPath: String? = null,
    // val rootPath: String? = null, // Usually determined by client/workspace
    // ... other settings related to preview, formatting, etc.
) 