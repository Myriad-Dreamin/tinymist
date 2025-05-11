package org.tinymist.intellij

import com.intellij.lang.HelpID
import com.intellij.lang.cacheBuilder.WordsScanner
import com.intellij.lang.findUsages.FindUsagesProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiNamedElement

class TypstFindUsagesProvider : FindUsagesProvider {

    override fun getWordsScanner(): WordsScanner? {
        // Delegate to LSP, so no client-side word scanning is typically needed.
        return null
    }

    override fun canFindUsagesFor(psiElement: PsiElement): Boolean {
        // Enable for named elements. LSP4IJ should determine if the specific element
        // at the given position can have references by querying the language server.
        // This can be refined later if more specific PSI elements are defined.
        return psiElement is PsiNamedElement
    }

    override fun getHelpId(psiElement: PsiElement): String? {
        return HelpID.FIND_OTHER_USAGES
    }

    override fun getType(element: PsiElement): String {
        // Generic type. The language server might provide more specific kinds.
        // This can be expanded if the plugin defines more PSI types.
        return "symbol"
    }

    override fun getDescriptiveName(element: PsiElement): String {
        return (element as? PsiNamedElement)?.name ?: element.text ?: ""
    }

    override fun getNodeText(element: PsiElement, useFullName: Boolean): String {
        return getDescriptiveName(element)
    }
} 