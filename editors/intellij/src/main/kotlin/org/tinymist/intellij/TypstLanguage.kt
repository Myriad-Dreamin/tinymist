package org.tinymist.intellij

import com.intellij.lang.Language
import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object TypstLanguage : Language("Typst")

object TypstFileType : LanguageFileType(TypstLanguage) {
    override fun getName(): String = "Typst file"
    override fun getDescription(): String = "Typst language file"
    override fun getDefaultExtension(): String = "typ"
    override fun getIcon(): Icon? = null // TODO: Add an icon
} 