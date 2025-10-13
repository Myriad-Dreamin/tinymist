package org.tinymist.intellij.settings

enum class ServerManagementMode {
    AUTO_MANAGE,    // Use installer to automatically manage the server
    CUSTOM_PATH     // Use user-specified custom path
}

object TinymistVersion {
    const val CURRENT = "v0.13.24"  // Centralized version definition
}

data class TinymistSettingsState(
    var tinymistExecutablePath: String = "",
    var serverManagementMode: ServerManagementMode = ServerManagementMode.AUTO_MANAGE
)