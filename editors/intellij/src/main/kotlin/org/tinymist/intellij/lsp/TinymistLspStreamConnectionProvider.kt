package org.tinymist.intellij.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import org.tinymist.intellij.settings.ServerManagementMode
import org.tinymist.intellij.settings.TinymistSettingsService
import java.io.File

class TinymistLspStreamConnectionProvider(@Suppress("unused") private val project: Project) : OSProcessStreamConnectionProvider() {

    companion object {
        private val LOG = Logger.getInstance(TinymistLspStreamConnectionProvider::class.java)
    }

    init {
        val settingsService = TinymistSettingsService.instance
        val serverManagementMode = settingsService.serverManagementMode
        var resolvedExecutablePath: String? = null

        when (serverManagementMode) {
            ServerManagementMode.CUSTOM_PATH -> {
                // Use custom path specified by user
                val customPath = settingsService.tinymistExecutablePath
                if (customPath.isNotBlank()) {
                    val customFile = File(customPath)
                    if (customFile.exists() && customFile.isFile && customFile.canExecute()) {
                        LOG.info("Using custom Tinymist executable path: $customPath")
                        resolvedExecutablePath = customPath
                    } else {
                        LOG.warn("Custom Tinymist path is invalid or not executable: $customPath")
                    }
                } else {
                    LOG.warn("Custom path mode selected but no path specified")
                }
                
                // If custom path fails, don't fall back to other methods - user explicitly chose custom
                if (resolvedExecutablePath == null) {
                    LOG.error("Custom path mode: Could not use specified Tinymist executable")
                }
            }
            
            ServerManagementMode.AUTO_MANAGE -> {
                resolvedExecutablePath = getInstallerManagedPath()
            }
        }
        
        // Only set commands if a valid executable path was resolved
        resolvedExecutablePath?.let {
            super.commandLine = GeneralCommandLine(it, "lsp")
        } ?: LOG.error("Tinymist LSP server commands not set as no executable was found.")
    }
    
    /**
     * Gets the path to the installer-managed Tinymist executable, if available.
     */
    private fun getInstallerManagedPath(): String? {
        return try {
            val installer = TinymistLanguageServerInstaller()
            installer.getInstalledExecutablePath()
        } catch (e: Exception) {
            LOG.warn("Failed to check installer-managed path: ${e.message}")
            null
        }
    }

    // TODO use initializeParams instead?
    //override fun getInitializationOptions(uri: VirtualFile?): Any {
    //    // Construct the nested Map structure directly
    //    val backgroundPreviewOpts = mapOf(
    //        "enabled" to true
    //        // "args" to listOf("--data-plane-host=127.0.0.1:23635", "--invert-colors=auto") // Example if needed
    //    )
    //    val previewOpts = mapOf(
    //        "background" to backgroundPreviewOpts
    //    )
//
    //    // Build the final options map
    //    // Add other top-level options expected by tinymist
    //    val options = mutableMapOf<String, Any>(
    //        "preview" to previewOpts,
    //        "semanticTokens" to mapOf<String, Any>(),
    //        "completion" to mapOf<String, Any>(),
    //        "lint" to mapOf<String, Any>()
    //        // Add other key-value pairs as needed
    //    )
//
    //    return options // Return the Map directly
    //}
}