package org.tinymist.intellij.structure

import com.intellij.ide.structureView.StructureViewModel
import com.intellij.ide.structureView.StructureViewModelBase
import com.intellij.ide.structureView.StructureViewTreeElement
import com.intellij.ide.util.treeView.smartTree.Sorter
import com.intellij.navigation.ItemPresentation
import com.intellij.openapi.editor.Editor
import com.intellij.psi.PsiFile
import org.tinymist.intellij.lsp.TinymistOutlineItem
import javax.swing.Icon

class TypstStructureViewModel(
    psiFile: PsiFile,
    editor: Editor?,
    private var rootOutlineItems: List<TinymistOutlineItem> // This will be updated by the LSP client
) : StructureViewModelBase(psiFile, editor, TypstStructureViewRootElement(psiFile, rootOutlineItems, psiFile.project)),
    StructureViewModel.ElementInfoProvider {

    init {
        // Enable an alphabetic sorter by default
        withSorters(Sorter.ALPHA_SORTER)
    }

    override fun isAlwaysShowsPlus(element: StructureViewTreeElement?): Boolean = false

    override fun isAlwaysLeaf(element: StructureViewTreeElement?): Boolean {
        return when (element) {
            is TypstStructureViewRootElement -> element.items.isEmpty() // Access items via the element instance
            is TypstStructureViewElement -> (element.value as? TinymistOutlineItem)?.children.isNullOrEmpty()
            else -> true // Default to leaf if type is unknown or value is not an outline item
        }
    }

    // This class represents the root of the structure view.
    // It holds the PsiFile for context but aims to be visually minimal.
    class TypstStructureViewRootElement(
        private val file: PsiFile,
        internal var items: List<TinymistOutlineItem>, // Made internal to be accessible from outer class's isAlwaysLeaf
        private val projectForElements: com.intellij.openapi.project.Project
    ) : StructureViewTreeElement, ItemPresentation {

        override fun getValue(): Any = file // Required by StructureViewModelBase

        override fun getPresentation(): ItemPresentation = this

        override fun getChildren(): Array<com.intellij.ide.util.treeView.smartTree.TreeElement> {
            val document = com.intellij.openapi.fileEditor.FileDocumentManager.getInstance().getDocument(file.virtualFile)
            return if (document != null) {
                items.map { TypstStructureViewElement(projectForElements, it, file, document) }.toTypedArray()
            } else {
                emptyArray()
            }
        }

        override fun navigate(requestFocus: Boolean) = Unit
        override fun canNavigate(): Boolean = false
        override fun canNavigateToSource(): Boolean = false

        // ItemPresentation for the (ideally invisible) root
        override fun getPresentableText(): String? = null // Make the root node textless
        override fun getLocationString(): String? = null
        override fun getIcon(unused: Boolean): Icon? = null // No icon for the root node
    }
} 