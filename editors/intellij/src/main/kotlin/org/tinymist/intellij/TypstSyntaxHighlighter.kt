package org.tinymist.intellij

import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.lexer.Lexer
import com.intellij.openapi.editor.colors.TextAttributesKey
import com.intellij.psi.tree.IElementType

class TypstSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter =
        TypstSyntaxHighlighter()
}

class TypstSyntaxHighlighter : com.intellij.openapi.fileTypes.SyntaxHighlighterBase() {
    // Keep this extremely simple for now. LSP provides semantic highlighting.
    // We might add basic keyword/comment highlighting later if desired.
    override fun getHighlightingLexer(): Lexer = TypstLexerAdapter()

    override fun getTokenHighlights(tokenType: IElementType?): Array<TextAttributesKey> {
        // Return empty array, relying on LSP for semantic tokens
        return TextAttributesKey.EMPTY_ARRAY
    }
} 