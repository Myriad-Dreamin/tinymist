package org.tinymist.intellij.settings

data class TinymistSettingsState(
    var tinymistExecutablePath: String = "",
    var enableAutoInstall: Boolean = true,
    var tinymistVersion: String = "v0.13.24",
    var useInstallerManagedBinary: Boolean = true
)