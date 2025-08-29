package org.tinymist.intellij.lsp

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification

/**
 * Custom Language Client to handle Tinymist-specific LSP notifications which will not be handled by LSP4IJ
 */
class TinymistLanguageClient(
    project: Project
) : LanguageClientImpl(project) {

    companion object {
        private val LOG = Logger.getInstance(TinymistLanguageClient::class.java)
    }

    @JsonNotification("tinymist/document")
    fun handleDocument(params: Any?) {
        // TODO: Replace Any with a specific data class if the structure of params is known.
        // For now, just log that the notification was received.
        System.err.println("TinymistLanguageClient: Received tinymist/document with params: ${'$'}params")
        // Future implementation: Update preview or other document-related views.
    }

    @JsonNotification("tinymist/documentOutline")
    fun handleDocumentOutline(params: Any?) {
        // TODO: Replace Any with the actual data class for outline parameters.
        // For now, just log receipt of the notification.
        LOG.info("Received tinymist/documentOutline notification with params: ${'$'}params")
        // Example of what might be done:
        // val outlineData = parseOutlineParams(params) // Implement parsing
        // ProjectActivity.getInstance(project).updateOutlineView(outlineData) // Example: if you have a way to update a view
    }
}
