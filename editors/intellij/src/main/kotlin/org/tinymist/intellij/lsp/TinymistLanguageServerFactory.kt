package org.tinymist.intellij.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.features.LSPClientFeatures
import com.redhat.devtools.lsp4ij.installation.ServerInstaller
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import org.eclipse.lsp4j.InitializeParams

class TinymistLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        return TinymistLspStreamConnectionProvider(project)
    }

    override fun createClientFeatures(): LSPClientFeatures {
        return object : LSPClientFeatures() {
            override fun initializeParams(initializeParams: InitializeParams) {
                val options = mapOf(
                    "preview" to mapOf(
                        "background" to mapOf(
                            "enabled" to true
                        )
                    )
                )
                initializeParams.initializationOptions = options
            }
        }.setDiagnosticFeature(TinymistLSPDiagnosticFeature())
    }

    override fun createServerInstaller(): ServerInstaller {
        return TinymistLanguageServerInstaller()
    }
}