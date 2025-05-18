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
import com.intellij.openapi.diagnostic.Logger
import org.tinymist.intellij.settings.TinymistSettingsService
import java.io.File

// Assuming your class looks something like this:
class TinymistLspStreamConnectionProvider(private val project: Project) : ProcessStreamConnectionProvider() {

    companion object {
        private val LOG = Logger.getInstance(TinymistLspStreamConnectionProvider::class.java)
        private const val TINYMIST_EXECUTABLE_NAME = "tinymist"
        // Ultimate fallback for development, should be removed or handled better in production
        private const val DEV_FALLBACK_PATH = "/Users/juliusschmitt/kotlin/tinymist/target/debug/tinymist"
    }

    init {
        val configuredPath = TinymistSettingsService.instance.tinymistExecutablePath
        var resolvedExecutablePath: String? = null

        if (configuredPath.isNotBlank()) {
            val configFile = File(configuredPath)
            if (configFile.exists() && configFile.isFile && configFile.canExecute()) {
                LOG.info("Using configured Tinymist executable path: $configuredPath")
                resolvedExecutablePath = configuredPath
            } else {
                LOG.warn("Configured Tinymist path is invalid or not executable: $configuredPath. Trying PATH.")
            }
        }

        if (resolvedExecutablePath == null) {
            resolvedExecutablePath = findExecutableOnPath(TINYMIST_EXECUTABLE_NAME)
            if (resolvedExecutablePath != null) {
                LOG.info("Found Tinymist executable on PATH: $resolvedExecutablePath")
            } else {
                LOG.warn("Tinymist executable not found in settings or on PATH. Trying development fallback.")
                // Development fallback - consider removing or improving this for production
                val devFallbackFile = File(DEV_FALLBACK_PATH)
                if (devFallbackFile.exists() && devFallbackFile.isFile && devFallbackFile.canExecute()) {
                    LOG.warn("Using DEVELOPMENT FALLBACK Tinymist executable path: $DEV_FALLBACK_PATH. Please configure the path in settings.")
                    resolvedExecutablePath = DEV_FALLBACK_PATH
                } else {
                    LOG.error("Tinymist executable not found. Please configure the path in 'Settings -> Tools -> Tinymist LSP' or ensure it's on your PATH. Development fallback also failed: $DEV_FALLBACK_PATH")
                    // Do not set commands if no valid executable is found.
                    // LSP server will not start, error is logged.
                }
            }
        }
        // Only set commands if a valid executable path was resolved
        resolvedExecutablePath?.let {
            super.setCommands(listOf(it, "lsp"))
        } ?: LOG.error("Tinymist LSP server commands not set as no executable was found.")
    }

    private fun findExecutableOnPath(name: String): String? {
        val systemPath = System.getenv("PATH")
        val pathDirs = systemPath?.split(File.pathSeparatorChar) ?: emptyList()
        for (dir in pathDirs) {
            val file = File(dir, name)
            if (file.exists() && file.isFile && file.canExecute()) {
                return file.absolutePath
            }
        }
        // Also check common variations for Windows if needed (e.g., .exe)
        if (System.getProperty("os.name").lowercase().contains("win")) {
            for (dir in pathDirs) {
                val file = File(dir, "$name.exe")
                if (file.exists() && file.isFile && file.canExecute()) {
                    return file.absolutePath
                }
            }
        }
        return null
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