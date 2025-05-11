package org.tinymist.intellij.lsp

import org.eclipse.lsp4j.Range

// Based on a common structure for document outlines. Adjusted for sparse data.

data class TinymistDocumentOutlineParams(
    val uri: String? = null, // URI of the document, if provided
    val items: List<TinymistOutlineItem> = emptyList()
)

data class TinymistOutlineItem(
    val name: String? = null, // Made nullable
    val kind: String? = null, // Made nullable. Could be an LSP SymbolKind (Int) or a custom string from Tinymist
    val detail: String? = null, // Optional additional details
    val range: Range? = null, // Made nullable. Full range of the element
    val selectionRange: Range? = null, // Made nullable. Range for selection (e.g., just the name)
    val children: List<TinymistOutlineItem>? = null
) 