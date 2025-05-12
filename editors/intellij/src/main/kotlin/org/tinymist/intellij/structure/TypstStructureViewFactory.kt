package org.tinymist.intellij.structure

import com.intellij.ide.structureView.StructureViewBuilder
import com.intellij.ide.structureView.StructureViewModel
import com.intellij.ide.structureView.TreeBasedStructureViewBuilder
import com.intellij.lang.PsiStructureViewFactory
import com.intellij.openapi.editor.Editor
import com.intellij.psi.PsiFile
import org.tinymist.intellij.TypstLanguage // Ensure this path is correct

class TypstStructureViewFactory : PsiStructureViewFactory {
    override fun getStructureViewBuilder(psiFile: PsiFile): StructureViewBuilder? {
        // Ensure the file is a Typst file.
        // Check TypstLanguage.INSTANCE.ID if TypstLanguage is an object with a companion ID
        // or psiFile.language is TypstLanguage if TypstLanguage is a class
        if (psiFile.language.id != TypstLanguage.id) { // Changed TypstLanguage.ID to TypstLanguage.id
            return null
        }

        return object : TreeBasedStructureViewBuilder() {
            override fun createStructureViewModel(editor: Editor?): StructureViewModel {
                return TypstStructureViewModel(psiFile, editor)
            }
        }
    }
} 