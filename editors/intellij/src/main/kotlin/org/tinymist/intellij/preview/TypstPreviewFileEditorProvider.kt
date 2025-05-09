package org.tinymist.intellij.preview

import com.intellij.openapi.fileEditor.FileEditor
import com.intellij.openapi.fileEditor.FileEditorPolicy
import com.intellij.openapi.fileEditor.FileEditorProvider
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile

class TypstPreviewFileEditorProvider : FileEditorProvider, DumbAware {
    override fun accept(project: Project, file: VirtualFile): Boolean {
        // This provider is used as the secondary (preview) editor within TypstTextEditorWithPreviewProvider.
        // The decision to open for a .typ file is handled by TypstTextEditorWithPreviewProvider.
        // So, if createEditor is called on this provider, we should accept.
        return true
    }

    override fun createEditor(project: Project, file: VirtualFile): FileEditor {
        return TypstPreviewFileEditor(project, file)
    }

    override fun getEditorTypeId(): String {
        // This should be a unique ID for this editor type.
        return "tinymist-preview-editor"
    }

    override fun getPolicy(): FileEditorPolicy {
        // Defines where the editor is placed relative to others, etc.
        return FileEditorPolicy.PLACE_AFTER_DEFAULT_EDITOR
    }
}