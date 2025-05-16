package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import org.eclipse.lsp4j.Diagnostic
import org.eclipse.lsp4j.PublishDiagnosticsParams
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import com.intellij.openapi.diagnostic.Logger
import org.eclipse.lsp4j.MessageActionItem
import org.eclipse.lsp4j.ShowMessageRequestParams
import java.util.concurrent.CompletableFuture

/**
 * Custom Language Client to handle Tinymist-specific LSP notifications.
 */
class TinymistLanguageClient(
    project: Project
) : LanguageClientImpl(project) {

    companion object {
        private val LOG = Logger.getInstance(TinymistLanguageClient::class.java)
    }

    override fun publishDiagnostics(diagnostics: PublishDiagnosticsParams) {
        val newDiagnostics = diagnostics.diagnostics.map { originalDiagnostic ->
            val originalMessage = originalDiagnostic.message
            // Replace newlines with <br> for proper tooltip rendering
            val newMessage = originalMessage.replace("\n", "<br>")

            val codeAsString: String? = when {
                originalDiagnostic.code == null -> null
                originalDiagnostic.code.isLeft -> originalDiagnostic.code.left
                originalDiagnostic.code.isRight -> originalDiagnostic.code.right.toString()
                else -> null // Should not happen for Either
            }

            // Use the 5-argument constructor (Range, Message, Severity, Source, Code)
            val newDiagnostic = Diagnostic(
                originalDiagnostic.range,
                newMessage,
                originalDiagnostic.severity,
                originalDiagnostic.source,
                codeAsString // Pass the processed code
            )

            // Set other properties if they exist, using setters
            originalDiagnostic.relatedInformation?.let { newDiagnostic.relatedInformation = it }
            originalDiagnostic.tags?.let { newDiagnostic.tags = it }
            originalDiagnostic.codeDescription?.let { newDiagnostic.codeDescription = it }
            originalDiagnostic.data?.let { newDiagnostic.data = it }

            newDiagnostic
        }
        super.publishDiagnostics(PublishDiagnosticsParams(diagnostics.uri, newDiagnostics))
    }

    @JsonNotification("tinymist/document")
    fun handleDocument(params: Any?) {
        // TODO: Replace Any with a specific data class if the structure of params is known.
        // For now, just log that the notification was received.
        System.err.println("TinymistLanguageClient: Received tinymist/document with params: ${'$'}params")
        // Future implementation: Update preview or other document-related views.
    }

    override fun showMessageRequest(params: ShowMessageRequestParams): CompletableFuture<MessageActionItem> {
        LOG.warn("Received showMessageRequest from server. Type: ${params.type}, Message: '${params.message}'. Actions: ${params.actions}. Suppressing UI and returning null action item to avoid potential NPE in lsp4ij.")
        // Returning a completed future with null, effectively ignoring the request from UI perspective
        // and preventing the lsp4ij default handler from causing an NPE if params.getActions() is null.
        return CompletableFuture.completedFuture(null)
    }
}
