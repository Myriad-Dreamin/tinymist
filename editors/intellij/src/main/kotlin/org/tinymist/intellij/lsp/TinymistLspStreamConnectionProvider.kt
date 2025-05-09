package org.tinymist.intellij.lsp

// Ensure these imports are present or adjust as needed
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider // Assuming this is the base class
// Remove imports for the old data classes if they are no longer used elsewhere
// import org.tinymist.intellij.lsp.BackgroundPreviewOptions
// import org.tinymist.intellij.lsp.PreviewOptions
// import org.tinymist.intellij.lsp.TinymistInitializationOptions

// Add other necessary imports
import com.intellij.openapi.vfs.VirtualFile // Added for the updated signature
import com.intellij.openapi.project.Project // Added for the constructor

// Assuming your class looks something like this:
class TinymistLspStreamConnectionProvider(private val project: Project) : ProcessStreamConnectionProvider() {

    init {
        // For now, assume tinymist is on the PATH
        val executablePath = "/Users/juliusschmitt/kotlin/tinymist/target/debug/tinymist"
        super.setCommands(listOf(executablePath, "lsp"))
    }

    override fun getInitializationOptions(uri: VirtualFile?): Any? {
        // Construct the nested Map structure directly
        val backgroundPreviewOpts = mapOf(
            "enabled" to true
            // "args" to listOf("--data-plane-host=127.0.0.1:23635", "--invert-colors=auto") // Example if needed
        )
        val previewOpts = mapOf(
            "background" to backgroundPreviewOpts
        )

        // Build the final options map
        // Add other top-level options expected by tinymist
        val options = mutableMapOf<String, Any>(
            "preview" to previewOpts,
            "semanticTokens" to mapOf<String, Any>(),
            "completion" to mapOf<String, Any>(),
            "lint" to mapOf<String, Any>()
            // Add other key-value pairs as needed
        )

        return options // Return the Map directly
    }

    // Add other necessary overrides like getWorkingDirectory()
    // override fun getWorkingDirectory(projectRoot: Path?): String? { ... }

    // Placeholder for executable finding logic
    // private fun findTinymistExecutable(): String { ... }
} 