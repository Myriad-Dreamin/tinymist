package org.tinymist.intellij

import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.psi.tree.IElementType
import com.intellij.openapi.editor.DefaultLanguageHighlighterColors

class TypstSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter =
        TypstSyntaxHighlighter()
}

class TypstSyntaxHighlighter : com.intellij.openapi.fileTypes.SyntaxHighlighterBase() {
    // Keep this extremely simple for now. LSP provides semantic highlighting.
    // We just need to handle the basic token type from our minimal lexer.
    override fun getHighlightingLexer(): Lexer = TypstLexerAdapter()

    override fun getTokenHighlights(tokenType: IElementType?): Array<TextAttributesKey> {
        return when (tokenType) {
            TYPST_TEXT -> TEXT_KEYS
            else -> EMPTY_KEYS
        }
    }

    companion object {
        private val TEXT_KEYS = arrayOf(DefaultLanguageHighlighterColors.IDENTIFIER)
        private val EMPTY_KEYS = arrayOf<TextAttributesKey>()
    }
} 