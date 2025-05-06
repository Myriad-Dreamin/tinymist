package org.tinymist.intellij

import com.intellij.extapi.psi.PsiFileBase
import com.intellij.openapi.fileTypes.FileType
import com.intellij.psi.FileViewProvider

class TypstFile(viewProvider: FileViewProvider) : PsiFileBase(viewProvider, TypstLanguage) {
    override fun getFileType(): FileType = TypstFileType
    override fun toString(): String = "Typst File"
} 