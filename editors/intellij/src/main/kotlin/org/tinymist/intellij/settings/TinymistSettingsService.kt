package org.tinymist.intellij.settings

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.util.xmlb.XmlSerializerUtil

@State(
    name = "org.tinymist.intellij.settings.TinymistSettingsState",
    storages = [Storage("tinymistSettings.xml")]
)
class TinymistSettingsService : PersistentStateComponent<TinymistSettingsState> {

    private var internalState = TinymistSettingsState()
    
    // Session-only port storage (not persisted across IDE restarts)
    @Volatile
    private var sessionPreviewPort: Int = 0

    companion object {
        val instance: TinymistSettingsService
            get() = ApplicationManager.getApplication().getService(TinymistSettingsService::class.java)
    }

    override fun getState(): TinymistSettingsState {
        return internalState
    }

    override fun loadState(state: TinymistSettingsState) {
        XmlSerializerUtil.copyBean(state, internalState)
    }

    // Convenience accessors for settings
    var tinymistExecutablePath: String
        get() = internalState.tinymistExecutablePath
        set(value) {
            internalState.tinymistExecutablePath = value
        }

    var serverManagementMode: ServerManagementMode
        get() = internalState.serverManagementMode
        set(value) {
            internalState.serverManagementMode = value
        }
    
    // Convenience methods for checking management mode
    val isAutoManaged: Boolean
        get() = serverManagementMode == ServerManagementMode.AUTO_MANAGE
        
    val isCustomPath: Boolean
        get() = serverManagementMode == ServerManagementMode.CUSTOM_PATH
    
    // Preview port management (session-only, not persisted)
    var previewPort: Int
        get() = sessionPreviewPort
        set(value) {
            sessionPreviewPort = value
        }


}