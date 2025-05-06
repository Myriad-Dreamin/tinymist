package org.tinymist.intellij

import com.intellij.lexer.FlexAdapter

// Using Flex for lexing might be overkill if LSP handles everything.
// Keep it minimal for now, potentially just for basic token types if needed.
class TypstLexerAdapter : FlexAdapter(_TypstLexer(null)) {
    // We would need a _TypstLexer.flex file if we wanted actual lexing.
    // For now, this structure satisfies the ParserDefinition.
}

// Placeholder for the Flex lexer class (generated from .flex file)
// If we don't create a .flex file, this won't actually do anything.
class _TypstLexer(reader: java.io.Reader?) : com.intellij.lexer.FlexLexer {
    override fun yybegin(state: Int) {}
    override fun yystate(): Int = 0
    override fun getTokenStart(): Int = 0
    override fun getTokenEnd(): Int = 0
    override fun advance(): com.intellij.psi.tree.IElementType? = null
    override fun reset(buf: CharSequence?, start: Int, end: Int, initialState: Int) {}
} 