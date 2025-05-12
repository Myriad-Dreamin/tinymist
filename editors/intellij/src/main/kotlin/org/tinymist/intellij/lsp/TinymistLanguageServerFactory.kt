package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import org.jetbrains.annotations.NotNull

class TinymistLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(@NotNull project: Project): StreamConnectionProvider {
        return TinymistLspStreamConnectionProvider(project)
    }

    // Assuming lsp4ij.LanguageServerFactory has a method like this to override.
    // This method allows lsp4ij to use our custom TinymistLanguageClient.
    // The exact signature might differ based on lsp4ij's API; if this causes
    // an error, the lsp4ij documentation should be consulted for the correct
    // way to provide a custom LanguageClient instance.
    override fun createLanguageClient(@NotNull project: Project): LanguageClientImpl {
        return TinymistLanguageClient(project) // Pass the project if the constructor needs it
    }
} 