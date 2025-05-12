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
import com.intellij.openapi.util.TextRange
import org.eclipse.lsp4j.Range

// Helper to convert LSP Range to IntelliJ TextRange
fun Range.toTextRange(document: Document): TextRange? {
    try {
        val startLine = this.start.line
        val startChar = this.start.character
        val endLine = this.end.line
        val endChar = this.end.character

        if (startLine < 0 || startLine >= document.lineCount || endLine < 0 || endLine >= document.lineCount) {
            // Invalid line numbers
            println("Error converting LSP range: Line numbers out of bounds. Start: $startLine, End: $endLine, Total lines: ${document.lineCount}")
            return null
        }

        val startOffset = document.getLineStartOffset(startLine) + startChar
        val endOffset = document.getLineStartOffset(endLine) + endChar
        
        if (startOffset > endOffset || startOffset < 0 || endOffset > document.textLength) {
             println("Error converting LSP range: Offsets out of bounds or invalid. Start: $startOffset, End: $endOffset, Length: ${document.textLength}")
            return null
        }
        return TextRange(startOffset, endOffset)
    } catch (e: IndexOutOfBoundsException) {
        println("Error converting LSP range to TextRange: ${e.message}. Range: Start(${this.start.line}, ${this.start.character}), End(${this.end.line}, ${this.end.character})")
    }
    return null
}

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
        val project = containingFile.project
        val virtualFile = containingFile.virtualFile ?: return

        // Use the document member of this class
        item.selectionRange?.toTextRange(document)?.let { textRange ->
             OpenFileDescriptor(project, virtualFile, textRange.startOffset).navigate(requestFocus)
        } ?: item.range?.toTextRange(document)?.let { textRange -> // Fallback to full range
            OpenFileDescriptor(project, virtualFile, textRange.startOffset).navigate(requestFocus)
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