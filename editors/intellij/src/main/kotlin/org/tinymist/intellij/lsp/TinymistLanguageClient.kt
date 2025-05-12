package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import org.eclipse.lsp4j.Diagnostic
import org.eclipse.lsp4j.PublishDiagnosticsParams
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import org.tinymist.intellij.structure.OutlineDataHolder
import java.net.URI
import java.nio.file.Paths
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

    @JsonNotification("tinymist/documentOutline")
    fun onDocumentOutline(params: TinymistDocumentOutlineParams) {
        // Log the entire params object structure for detailed inspection (can be verbose)
        // LOG.info("Received tinymist/documentOutline notification. Full Params: $params")

        val uriString = params.uri
        val items = params.items

        if (uriString == null) {
            // TODO: SERVER-SIDE FIX REQUIRED: The Tinymist LSP server must send a valid 'uri' string
            // in the 'tinymist/documentOutline' notification parameters. Without a URI,
            // the client cannot associate the outline with a specific document.
            // Currently, this results in the Structure View showing mock data as a fallback.
            val itemsSummary = if (items == null) "null" else "count=${items.size}"
            LOG.warn("TinymistLanguageClient: Received documentOutline without a URI. Items: $itemsSummary. OutlineDataHolder will not be updated for any specific file; mock data may be shown.")
            if (items != null && items.isNotEmpty()) {
                LOG.warn("First few items (up to 3) from outline with null URI: " +
                        items.take(3).joinToString { "Name: '${it.name ?: "null"}', Detail: '${it.detail ?: "null"}', Children: ${it.children?.size ?: 0}" })
            }
            return // Do not update OutlineDataHolder if URI is missing
        }

        try {
            val parsedUri = URI(uriString)
            if (parsedUri.scheme != "file") {
                LOG.warn("Received documentOutline with non-file URI scheme: $uriString. Items count: ${items?.size ?: "null"}")
                return
            }
            // Convert file URI to a normalized path string
            val filePath = Paths.get(parsedUri).toString()
            LOG.info("Updating outline for filePath: '$filePath' with ${items?.size ?: 0} items.")
            // Ensure items is not null before passing to updateOutline; provide emptyList if it is.
            OutlineDataHolder.updateOutline(filePath, items ?: emptyList())
        } catch (e: Exception) {
            LOG.error("Error processing documentOutline for URI '$uriString': ${e.message}", e)
        }
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
