package org.tinymist.intellij.lsp

import com.redhat.devtools.lsp4ij.client.features.LSPDiagnosticFeature
import org.eclipse.lsp4j.Diagnostic

/**
 * Custom LSP diagnostic feature for Tinymist language server.
 * 
 * This feature customizes the diagnostic message display to properly handle
 * multi-line diagnostic messages by converting newlines to HTML line breaks
 * for better tooltip rendering in IntelliJ.
 */
class TinymistLSPDiagnosticFeature : LSPDiagnosticFeature() {
    
    override fun getMessage(diagnostic: Diagnostic): String {
        // Replace newlines with <br> for proper tooltip rendering in HTML
        return diagnostic.message.replace("\n", "<br>")
    }
}