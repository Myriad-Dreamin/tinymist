package org.tinymist.intellij.preview

import com.intellij.openapi.fileEditor.FileEditor
import com.intellij.openapi.fileEditor.FileEditorProvider
import com.intellij.openapi.fileEditor.FileEditorPolicy
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.project.DumbAware

class TypstPreviewFileEditorProvider : FileEditorProvider, DumbAware {
    override fun accept(project: Project, file: VirtualFile): Boolean = true // Accept all files for preview; filter if needed

    override fun createEditor(project: Project, file: VirtualFile): FileEditor {
        return TypstPreviewFileEditor(project, file)
    }

    override fun getEditorTypeId(): String = "tinymist-preview-editor"

    override fun getPolicy(): FileEditorPolicy = FileEditorPolicy.PLACE_AFTER_DEFAULT_EDITOR
} 