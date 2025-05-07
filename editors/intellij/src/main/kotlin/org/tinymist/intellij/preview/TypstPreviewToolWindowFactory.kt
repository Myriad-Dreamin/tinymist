package org.tinymist.intellij.preview

import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import javax.swing.JLabel
import javax.swing.JPanel

class TypstPreviewToolWindowFactory : ToolWindowFactory, DumbAware {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val previewPanel = TypstPreviewPanel()
        val contentFactory = ContentFactory.getInstance()
        val content = contentFactory.createContent(previewPanel.component, "", false)
        toolWindow.contentManager.addContent(content)
    }

    override fun shouldBeAvailable(project: Project) = true
}

// A simple class to hold our preview UI. This will eventually host the JCEF browser.
class TypstPreviewPanel {
    val component: JPanel = JPanel().apply {
        add(JLabel("Typst Preview Will Be Here (JCEF Placeholder)"))
    }
    // We will add JCEF initialization and browser component here later
} 