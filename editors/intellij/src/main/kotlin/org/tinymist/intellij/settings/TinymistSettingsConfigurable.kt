package org.tinymist.intellij.settings

import com.intellij.openapi.options.Configurable
import com.intellij.openapi.project.ProjectManager
import com.intellij.openapi.diagnostic.Logger
import com.redhat.devtools.lsp4ij.LanguageServerManager
import com.redhat.devtools.lsp4ij.LanguageServersRegistry
import com.redhat.devtools.lsp4ij.LanguageServerManager.StopOptions
import com.redhat.devtools.lsp4ij.LanguageServiceAccessor
import com.redhat.devtools.lsp4ij.ServerStatus
import com.redhat.devtools.lsp4ij.server.definition.LanguageServerDefinition
import com.redhat.devtools.lsp4ij.server.definition.LanguageServerDefinitionListener.LanguageServerChangedEvent
import javax.swing.JComponent

class TinymistSettingsConfigurable : Configurable {

    private var settingsPanel: TinymistSettingsPanel? = null
    private val settingsService = TinymistSettingsService.instance

    companion object {
        private val LOG = Logger.getInstance(TinymistSettingsConfigurable::class.java)
        private const val TINYMIST_SERVER_ID = "tinymistServer"
    }

    override fun getDisplayName(): String = "Tinymist LSP"

    override fun getHelpTopic(): String? = null

    override fun createComponent(): JComponent? {
        settingsPanel = TinymistSettingsPanel()
        return settingsPanel?.mainPanel
    }

    override fun isModified(): Boolean {
        return settingsPanel?.tinymistExecutablePath != settingsService.tinymistExecutablePath
    }

    override fun apply() {
        val currentSettingsPath = settingsService.state.tinymistExecutablePath
        val newPanelPath = settingsPanel?.tinymistExecutablePathField?.text ?: ""

        val pathChanged = currentSettingsPath != newPanelPath

        // Always update the settings state with the panel's current value
        settingsService.state.tinymistExecutablePath = newPanelPath

        if (pathChanged) {
            LOG.info("Tinymist executable path changed. Old: '$currentSettingsPath', New: '$newPanelPath'. Requesting server restart.")

            val registry = LanguageServersRegistry.getInstance()
            val serverDefinition = registry.getServerDefinition(TINYMIST_SERVER_ID)

            if (serverDefinition != null) {
                LOG.debug("Found server definition: $serverDefinition for ID $TINYMIST_SERVER_ID")

                ProjectManager.getInstance().openProjects.forEach { project ->
                    if (!project.isDisposed && project.isOpen) {
                        // Construct and fire the LanguageServerChangedEvent
                        val event = LanguageServerChangedEvent(
                            project,       // current project
                            serverDefinition, // the definition of our server
                            false,         // nameChanged
                            true,          // commandChanged - THIS IS KEY
                            false,         // userEnvironmentVariablesChanged
                            false,         // includeSystemEnvironmentVariablesChanged
                            false,         // mappingsChanged
                            false,         // configurationContentChanged
                            false,         // initializationOptionsContentChanged
                            false          // clientConfigurationContentChanged
                        )
                        registry.handleChangeEvent(event) // Notify lsp4ij about the change
                        LOG.info("Fired LanguageServerChangedEvent for project: ${project.name}. lsp4ij should handle server restart.")
                    }
                }
            } else {
                LOG.warn("Could not find server definition for ID $TINYMIST_SERVER_ID. Server restart will not be automatically triggered.")
            }
        }
    }

    override fun reset() {
        settingsPanel?.tinymistExecutablePath = settingsService.tinymistExecutablePath
    }

    override fun disposeUIResources() {
        settingsPanel = null
    }
} 