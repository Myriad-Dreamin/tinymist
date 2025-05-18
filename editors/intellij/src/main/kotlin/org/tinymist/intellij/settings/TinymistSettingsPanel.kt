package org.tinymist.intellij.settings

import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.dsl.builder.panel
import com.intellij.ui.dsl.builder.AlignX
import javax.swing.JPanel

class TinymistSettingsPanel {
    val mainPanel: JPanel
    val tinymistExecutablePathField = TextFieldWithBrowseButton()

    init {
        tinymistExecutablePathField.addBrowseFolderListener(
            "Select Tinymist Executable",
            null,
            null,
            FileChooserDescriptorFactory.createSingleFileOrExecutableAppDescriptor()
        )

        mainPanel = panel {
            row("Tinymist executable path:") {
                cell(tinymistExecutablePathField)
                    .resizableColumn()
                    .align(AlignX.FILL)
            }
        }
    }

    var tinymistExecutablePath: String
        get() = tinymistExecutablePathField.text
        set(value) {
            tinymistExecutablePathField.text = value
        }
} 