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
import com.intellij.openapi.fileEditor.FileDocumentManager

class TypstStructureViewModel(
    psiFile: PsiFile,
    editor: Editor?
) : StructureViewModelBase(psiFile, editor, TypstStructureViewRootElement(psiFile)),
    StructureViewModel.ElementInfoProvider {

    init {
        // TODO: Setup listener for OutlineDataHolder updates if needed.
        // For example, using project.messageBus connect and subscribe to a topic
        // that OutlineDataHolder.updateOutline publishes to. On update, call:
        // super.fireTreeChanged(true) or similar to refresh the whole tree.
    }

    override fun getSorters(): Array<Sorter> = Sorter.EMPTY_ARRAY

    // For isAlwaysShowsPlus and isAlwaysLeaf, we rely on the defaults from StructureViewModelBase
    // or provide logic based on the element if necessary.
    override fun isAlwaysShowsPlus(element: StructureViewTreeElement): Boolean = false

    override fun isAlwaysLeaf(element: StructureViewTreeElement): Boolean {
        // An element is a leaf if it has no children.
        // This can be checked by casting and inspecting, or if your elements store this info.
        return element is TypstStructureViewElement && element.value is TinymistOutlineItem && element.children.isEmpty()
    }

    // This class represents the root of the structure view.
    // It holds the PsiFile for context but aims to be visually minimal.
    class TypstStructureViewRootElement(private val psiFile: PsiFile) : StructureViewTreeElement, ItemPresentation {

        override fun getValue(): Any = psiFile

        override fun getPresentableText(): String? = psiFile.name
        override fun getLocationString(): String? = null
        override fun getIcon(unused: Boolean): javax.swing.Icon? = psiFile.getIcon(0)

        override fun getPresentation(): ItemPresentation = this

        override fun getChildren(): Array<StructureViewTreeElement> {
            val project = psiFile.project
            val virtualFile = psiFile.virtualFile ?: return emptyArray()
            val document = FileDocumentManager.getInstance().getDocument(virtualFile) ?: return emptyArray()

            val filePath = virtualFile.path
            val outlineItems = OutlineDataHolder.getOutline(filePath)
            return outlineItems.mapNotNull { item ->
                TypstStructureViewElement(project, item, psiFile, document)
            }.toTypedArray()
        }

        override fun navigate(requestFocus: Boolean) {
            (psiFile as? com.intellij.pom.Navigatable)?.navigate(requestFocus)
        }

        override fun canNavigate(): Boolean = (psiFile as? com.intellij.pom.Navigatable)?.canNavigate() == true

        override fun canNavigateToSource(): Boolean = canNavigate()
    }
} 