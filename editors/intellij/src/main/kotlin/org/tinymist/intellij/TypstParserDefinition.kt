package org.tinymist.intellij

import com.intellij.lang.ASTNode
import com.intellij.lang.ParserDefinition
import com.intellij.lang.PsiParser
import com.intellij.lexer.Lexer
import com.intellij.openapi.project.Project
import com.intellij.psi.FileViewProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IFileElementType
import com.intellij.psi.tree.TokenSet
import com.intellij.extapi.psi.ASTWrapperPsiElement
import com.intellij.psi.tree.IElementType

// Define the missing IElementType
val TYPST_TEXT: IElementType = IElementType("TYPST_TEXT", TypstLanguage)

// Basic implementation - LSP handles most things
class TypstParserDefinition : ParserDefinition {
    private val file = IFileElementType(TypstLanguage)

    override fun createLexer(project: Project?): Lexer = TypstLexerAdapter()

    override fun createParser(project: Project?): PsiParser {
        // We won't be doing heavy PSI parsing here, LSP handles structure
        return PsiParser { _, builder ->
            val rootMarker = builder.mark()
            // Consume entire input as a single file node
            while (!builder.eof()) {
                builder.advanceLexer()
            }
            rootMarker.done(file)
            builder.treeBuilt // Return the built ASTNode
        }
    }

    override fun getFileNodeType(): IFileElementType = file

    override fun getCommentTokens(): TokenSet = TokenSet.EMPTY // TODO: Define comment tokens if needed for basic highlighting

    override fun getStringLiteralElements(): TokenSet = TokenSet.EMPTY // TODO: Define string tokens

    override fun createFile(viewProvider: FileViewProvider): PsiFile = TypstFile(viewProvider)

    override fun createElement(node: ASTNode): PsiElement {
        // Use ASTWrapperPsiElement as a generic wrapper since we don't have specific element types
        return ASTWrapperPsiElement(node)
    }
} 