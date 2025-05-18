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

    // Convenience accessor if needed, or use instance.state.tinymistExecutablePath directly
    var tinymistExecutablePath: String
        get() = internalState.tinymistExecutablePath
        set(value) {
            internalState.tinymistExecutablePath = value
        }
} 