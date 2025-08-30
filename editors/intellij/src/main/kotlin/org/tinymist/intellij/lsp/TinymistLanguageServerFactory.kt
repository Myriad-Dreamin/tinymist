package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.features.LSPClientFeatures
import com.redhat.devtools.lsp4ij.installation.ServerInstaller
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class TinymistLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        return TinymistLspStreamConnectionProvider(project)
    }
    
    override fun createClientFeatures(): LSPClientFeatures {
        return LSPClientFeatures()
            .setDiagnosticFeature(TinymistLSPDiagnosticFeature())
    }
    
    override fun createServerInstaller(): ServerInstaller {
        return TinymistLanguageServerInstaller()
    }
}