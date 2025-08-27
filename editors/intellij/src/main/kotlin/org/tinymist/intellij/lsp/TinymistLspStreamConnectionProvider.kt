package org.tinymist.intellij.lsp

import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider // Assuming this is the base class
// Removes imports for the old data classes if they are no longer used elsewhere
// import org.tinymist.intellij.lsp.BackgroundPreviewOptions
// import org.tinymist.intellij.lsp.PreviewOptions
// import org.tinymist.intellij.lsp.TinymistInitializationOptions

// Adds other necessary imports
import com.intellij.openapi.vfs.VirtualFile // Required for the updated signature
import com.intellij.openapi.project.Project // Required for the constructor
import com.intellij.openapi.diagnostic.Logger
import org.tinymist.intellij.settings.TinymistSettingsService
import java.io.File

class TinymistLspStreamConnectionProvider(private val project: Project) : ProcessStreamConnectionProvider() {

    companion object {
        private val LOG = Logger.getInstance(TinymistLspStreamConnectionProvider::class.java)
        private const val TINYMIST_EXECUTABLE_NAME = "tinymist"
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
                LOG.error("Could not find Tinymist executable on PATH.")
            }
        }
        // Sets commands only if a valid executable path was resolved
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
        // Checks common variations for Windows if needed (e.g., .exe)
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
        val backgroundPreviewOpts = mapOf(
            "enabled" to true
        )
        val previewOpts = mapOf(
            "background" to backgroundPreviewOpts
        )

        // Adds other top-level options expected by tinymist
        val options = mutableMapOf<String, Any>(
            "preview" to previewOpts,
            "semanticTokens" to mapOf<String, Any>(),
            "completion" to mapOf<String, Any>(),
            "lint" to mapOf<String, Any>()
        )

        return options
    }
} 