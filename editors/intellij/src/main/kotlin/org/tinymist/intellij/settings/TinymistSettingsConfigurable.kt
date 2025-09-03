package org.tinymist.intellij.settings

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.options.Configurable
import com.intellij.openapi.project.ProjectManager
import com.redhat.devtools.lsp4ij.LanguageServersRegistry
import com.redhat.devtools.lsp4ij.server.definition.LanguageServerDefinitionListener.LanguageServerChangedEvent
import com.redhat.devtools.lsp4ij.server.definition.LanguageServerDefinitionListener.LanguageServerDefinitionEvent
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
        val panel = settingsPanel ?: return false
        return panel.tinymistExecutablePath != settingsService.tinymistExecutablePath ||
               panel.serverManagementMode != settingsService.serverManagementMode
    }

    override fun apply() {
        val panel = settingsPanel ?: return
        
        val currentSettingsPath = settingsService.state.tinymistExecutablePath
        val currentManagementMode = settingsService.state.serverManagementMode
        val newPanelPath = panel.tinymistExecutablePath
        val newManagementMode = panel.serverManagementMode

        val pathChanged = currentSettingsPath != newPanelPath
        val modeChanged = currentManagementMode != newManagementMode

        // Always update the settings state with the panel's current values
        settingsService.state.tinymistExecutablePath = newPanelPath
        settingsService.state.serverManagementMode = newManagementMode

        if (pathChanged || modeChanged) {
            LOG.info("Tinymist settings changed. Path: '$currentSettingsPath' -> '$newPanelPath', Mode: '$currentManagementMode' -> '$newManagementMode'. Requesting server restart.")

            val registry = LanguageServersRegistry.getInstance()
            val serverDefinition = registry.getServerDefinition(TINYMIST_SERVER_ID)

            if (serverDefinition != null) {
                LOG.debug("Found server definition: $serverDefinition for ID $TINYMIST_SERVER_ID")

                ProjectManager.getInstance().openProjects.forEach { project ->
                    if (!project.isDisposed && project.isOpen) {
                        // Construct and fire the LanguageServerChangedEvent
                        val event = LanguageServerChangedEvent(
                            LanguageServerDefinitionEvent.UpdatedBy.USER, // who triggered the update
                            project,       // current project
                            serverDefinition, // the definition of our server
                            false,         // nameChanged
                            true,          // commandChanged - THIS IS KEY
                            false,         // userEnvironmentVariablesChanged
                            false,         // includeSystemEnvironmentVariablesChanged
                            false,         // mappingsChanged
                            false,         // clientConfigurationContentChanged
                            false          // installerConfigurationContentChanged
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
        val panel = settingsPanel ?: return
        panel.tinymistExecutablePath = settingsService.tinymistExecutablePath
        panel.serverManagementMode = settingsService.serverManagementMode
    }

    override fun disposeUIResources() {
        settingsPanel = null
    }
} 