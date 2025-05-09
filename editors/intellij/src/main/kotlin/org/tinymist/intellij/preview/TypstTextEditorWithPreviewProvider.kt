package org.tinymist.intellij.preview

import com.intellij.openapi.fileEditor.TextEditorWithPreviewProvider
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import org.tinymist.intellij.TypstFileType

/**
 * This is the main provider registered in plugin.xml for Typst files.
 * It combines a standard text editor (PsiAwareTextEditorProvider) with our custom preview editor (TypstPreviewFileEditor).
 */
class TypstTextEditorWithPreviewProvider : TextEditorWithPreviewProvider(
    TypstPreviewFileEditorProvider() // Changed to use the new standalone provider
), DumbAware {

    /**
     * Determines whether this TextEditorWithPreviewProvider should handle the given file.
     * We only want to provide this combined editor for Typst files.
     */
    override fun accept(project: Project, file: VirtualFile): Boolean {
        return file.fileType is TypstFileType || file.extension?.equals(TypstFileType.defaultExtension, ignoreCase = true) == true
    }
} 