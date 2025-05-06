package org.tinymist.intellij

import com.intellij.lexer.FlexAdapter
import com.intellij.lexer.Lexer
import com.intellij.lexer.LexerPosition
import com.intellij.psi.tree.IElementType
import com.intellij.util.text.CharArrayUtil

// Basic lexer implementation required by the platform.
class TypstLexerAdapter : Lexer() {
    private var buffer: CharSequence = ""
    private var startOffset: Int = 0
    private var endOffset: Int = 0
    private var currentOffset: Int = 0
    private var currentToken: IElementType? = null
    private var currentState: Int = 0 // Added state tracking

    // Implement LexerPosition for getCurrentPosition
    private class TypstLexerPosition(private val offset: Int, private val state: Int) : LexerPosition {
        override fun getOffset(): Int = offset
        override fun getState(): Int = state
    }

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        this.buffer = buffer
        this.startOffset = startOffset
        this.endOffset = endOffset
        this.currentOffset = startOffset
        this.currentState = initialState
        advance() // Prime the first token
    }

    override fun getState(): Int = currentState

    override fun getTokenType(): IElementType? = currentToken

    override fun getTokenStart(): Int = startOffset

    override fun getTokenEnd(): Int = endOffset

    override fun advance() {
        startOffset = currentOffset // Update start offset for the next token
        if (currentOffset < endOffset) {
            // Consume the whole buffer as one token type
            currentToken = TYPST_TEXT
            currentOffset = endOffset // Mark as consumed
        } else {
            currentToken = null // End of buffer
        }
        // Our simple lexer doesn't change state, but we store it
        currentState = 0 
    }

    override fun getCurrentPosition(): LexerPosition {
        return TypstLexerPosition(currentOffset, currentState)
    }

    override fun restore(position: LexerPosition) {
        currentOffset = position.offset
        currentState = position.state
        // Need to re-prime the token based on the restored position
        startOffset = currentOffset // Start of the potential next token is the restored offset
        if (currentOffset < endOffset) {
            currentToken = TYPST_TEXT
        } else {
            currentToken = null
        }
    }

    override fun getBufferSequence(): CharSequence = buffer

    override fun getBufferEnd(): Int = endOffset
}

// Placeholder for the Flex lexer class (generated from .flex file)
// This is no longer used directly by TypstLexerAdapter but kept for potential future Flex integration.
class _TypstLexer(reader: java.io.Reader?) : com.intellij.lexer.FlexLexer {
    // Parameter 'reader' is never used - This warning is expected for the placeholder
    override fun yybegin(state: Int) {}
    override fun yystate(): Int = 0
    override fun getTokenStart(): Int = 0
    override fun getTokenEnd(): Int = 0
    override fun advance(): com.intellij.psi.tree.IElementType? = null
    override fun reset(buf: CharSequence?, start: Int, end: Int, initialState: Int) {}
} 