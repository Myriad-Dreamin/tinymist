package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.installation.ServerInstaller
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import org.jetbrains.annotations.NotNull
import org.jetbrains.annotations.Nullable

class TinymistLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(@NotNull project: Project): StreamConnectionProvider {
        return TinymistLspStreamConnectionProvider(project)
    }

    override fun createLanguageClient(@NotNull project: Project): LanguageClientImpl {
        return TinymistLanguageClient(project)
    }
    
    @Nullable
    override fun createServerInstaller(): ServerInstaller {
        return TinymistLanguageServerInstaller()
    }
}