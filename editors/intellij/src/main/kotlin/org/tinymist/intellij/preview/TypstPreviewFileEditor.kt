package org.tinymist.intellij.preview

import com.intellij.openapi.fileEditor.FileEditor
import com.intellij.openapi.fileEditor.FileEditorLocation
import com.intellij.openapi.fileEditor.FileEditorState
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.util.UserDataHolderBase
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JBCefBrowser
import java.beans.PropertyChangeListener
import javax.swing.JComponent
import javax.swing.JLabel
import javax.swing.JPanel
import com.intellij.openapi.fileEditor.FileEditorPolicy
import com.intellij.openapi.fileEditor.FileEditorProvider
import com.intellij.openapi.project.DumbAware
import org.tinymist.intellij.TypstFileType

class TypstPreviewFileEditor(
    private val project: Project, // Keep project for potential future use (e.g., settings)
    private val virtualFile: VirtualFile
) : UserDataHolderBase(), FileEditor {

    private var jbCefBrowser: JBCefBrowser? = null
    private val panel: JComponent

    init {
        if (JBCefApp.isSupported()) {
            jbCefBrowser = JBCefBrowser()
            // Placeholder content for now
            jbCefBrowser?.loadHTML("<html><body><h1>Typst Preview (JCEF)</h1><p>File: ${virtualFile.name}</p><p>This is a placeholder. Real preview content will be loaded from the Tinymist LSP server.</p></body></html>")
            panel = jbCefBrowser!!.component
        } else {
            // Fallback if JCEF is not supported
            panel = JPanel().apply {
                add(JLabel("JCEF browser is not supported in this environment. Typst preview cannot be displayed."))
            }
        }
    }

    override fun getComponent(): JComponent = panel

    override fun getPreferredFocusedComponent(): JComponent? = panel

    override fun getName(): String = "Typst Preview"

    override fun setState(state: FileEditorState) {
        // TODO: Handle state persistence if needed (e.g., scroll position)
    }

    override fun isModified(): Boolean = false

    override fun isValid(): Boolean = virtualFile.isValid

    override fun addPropertyChangeListener(listener: PropertyChangeListener) {}

    override fun removePropertyChangeListener(listener: PropertyChangeListener) {}

    override fun getCurrentLocation(): FileEditorLocation? = null

    override fun dispose() {
        jbCefBrowser?.let { Disposer.dispose(it) }
    }

    override fun getFile(): VirtualFile = virtualFile

    fun updateContent(htmlContent: String) {
        jbCefBrowser?.loadHTML(htmlContent)
    }

    fun loadURL(url: String) {
        jbCefBrowser?.loadURL(url)
    }

    // Companion object to act as the FileEditorProvider for this preview editor
    class Provider : FileEditorProvider, DumbAware {
        override fun accept(project: Project, file: VirtualFile): Boolean {
            // This provider is specifically for Typst files when used as a preview component
            return file.fileType is TypstFileType || file.extension?.equals(TypstFileType.defaultExtension, ignoreCase = true) == true
        }

        override fun createEditor(project: Project, file: VirtualFile): FileEditor {
            return TypstPreviewFileEditor(project, file)
        }

        override fun getEditorTypeId(): String {
            return "typst.preview.file.editor" // Unique ID for this specific editor type
        }

        override fun getPolicy(): FileEditorPolicy {
            // This policy is important. PLACE_AFTER_DEFAULT_EDITOR is often suitable for previews
            // that are part of a TextEditorWithPreviewProvider setup.
            return FileEditorPolicy.PLACE_AFTER_DEFAULT_EDITOR
        }
    }
} 