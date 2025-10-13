package org.tinymist.intellij.settings

import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.ui.dsl.builder.AlignX
import com.intellij.ui.dsl.builder.panel
import javax.swing.ButtonGroup
import javax.swing.JPanel
import javax.swing.JRadioButton

class TinymistSettingsPanel {
    val mainPanel: JPanel
    val tinymistExecutablePathField = TextFieldWithBrowseButton()
    
    private val autoManageRadio = JRadioButton("Auto-manage Tinymist server (recommended)")
    private val customPathRadio = JRadioButton("Use custom Tinymist executable")
    private val buttonGroup = ButtonGroup()

    init {
        buttonGroup.add(autoManageRadio)
        buttonGroup.add(customPathRadio)
        
        // Default to auto-manage
        autoManageRadio.isSelected = true
        
        // Configure file chooser using the correct API
        tinymistExecutablePathField.addActionListener {
            val fileChooser = FileChooserDescriptorFactory.createSingleFileOrExecutableAppDescriptor()
            fileChooser.title = "Select Tinymist Executable"
            // File chooser will be handled by the TextFieldWithBrowseButton automatically
        }

        // Enable/disable path field based on radio button selection
        autoManageRadio.addActionListener { 
            tinymistExecutablePathField.isEnabled = false
        }
        customPathRadio.addActionListener { 
            tinymistExecutablePathField.isEnabled = true
        }

        mainPanel = panel {
            @Suppress("DialogTitleCapitalization")
            buttonsGroup("Server Management") {
                row {
                    cell(autoManageRadio)
                }
                row {
                    text("The plugin will automatically download and manage the Tinymist server binary.")
                        .apply { component.font = component.font.deriveFont(component.font.size - 1f) }
                }
                row {
                    cell(customPathRadio)
                }
                row("Executable path:") {
                    cell(tinymistExecutablePathField)
                        .resizableColumn()
                        .align(AlignX.FILL)
                }
                row {
                    text("Specify the path to your own Tinymist executable.")
                        .apply { component.font = component.font.deriveFont(component.font.size - 1f) }
                }
            }
        }
        
        // Initially disable path field since auto-manage is selected by default
        tinymistExecutablePathField.isEnabled = false
    }

    var tinymistExecutablePath: String
        get() = tinymistExecutablePathField.text
        set(value) {
            tinymistExecutablePathField.text = value
        }
        
    var serverManagementMode: ServerManagementMode
        get() = if (autoManageRadio.isSelected) ServerManagementMode.AUTO_MANAGE else ServerManagementMode.CUSTOM_PATH
        set(value) {
            when (value) {
                ServerManagementMode.AUTO_MANAGE -> {
                    autoManageRadio.isSelected = true
                    tinymistExecutablePathField.isEnabled = false
                }
                ServerManagementMode.CUSTOM_PATH -> {
                    customPathRadio.isSelected = true
                    tinymistExecutablePathField.isEnabled = true
                }
            }
        }
}