package org.tinymist.intellij.structure

import com.intellij.ide.structureView.StructureViewTreeElement
import com.intellij.ide.util.treeView.smartTree.SortableTreeElement
import com.intellij.ide.util.treeView.smartTree.TreeElement
import com.intellij.navigation.ItemPresentation
import com.intellij.navigation.NavigationItem
import com.intellij.openapi.editor.Document
import com.intellij.openapi.fileEditor.OpenFileDescriptor
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.pom.Navigatable
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.PsiManager
import com.intellij.psi.util.PsiTreeUtil
import org.tinymist.intellij.lsp.TinymistOutlineItem
import javax.swing.Icon

class TypstStructureViewElement(
    private val project: Project,
    private val item: TinymistOutlineItem,
    private val containingFile: PsiFile,
    private val document: Document
) : StructureViewTreeElement, SortableTreeElement, NavigationItem {

    override fun getValue(): Any = item

    override fun getPresentation(): ItemPresentation {
        return object : ItemPresentation {
            override fun getPresentableText(): String? = item.name ?: "<unnamed>"
            override fun getLocationString(): String? = item.detail
            override fun getIcon(unused: Boolean): Icon? = null // TODO: Map item.kind to an icon
        }
    }

    override fun getChildren(): Array<TreeElement> {
        return item.children?.map { TypstStructureViewElement(project, it, containingFile, document) }?.toTypedArray() ?: emptyArray()
    }

    // NavigationItem & Navigatable implementation
    override fun navigate(requestFocus: Boolean) {
        val range = item.selectionRange ?: item.range ?: return
        val startOffset = getOffset(document, range.start.line, range.start.character)
        // val endOffset = getOffset(document, range.end.line, range.end.character)
        if (startOffset != -1) {
            OpenFileDescriptor(project, containingFile.virtualFile, startOffset).navigate(requestFocus)
        }
    }

    override fun canNavigate(): Boolean {
        return (item.selectionRange != null || item.range != null) && containingFile.virtualFile != null
    }

    override fun canNavigateToSource(): Boolean = canNavigate()

    override fun getName(): String? = item.name

    // SortableTreeElement
    override fun getAlphaSortKey(): String = item.name ?: ""

    // Helper to convert LSP line/character to offset
    private fun getOffset(document: Document, line: Int, char: Int): Int {
        if (line < 0 || line >= document.lineCount) return -1
        val lineStartOffset = document.getLineStartOffset(line)
        // LSP char is 0-indexed on the line, while IntelliJ columns can be 1-indexed by some APIs.
        // Assuming char is a direct character count on the line.
        val offset = lineStartOffset + char
        return if (offset <= document.textLength) offset else -1
    }
} 