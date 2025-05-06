package org.tinymist.intellij.lsp

import com.intellij.execution.ExecutionException
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor
import org.tinymist.intellij.TypstFileType

class TinymistLspServerSupportProvider : LspServerSupportProvider {
    override fun fileOpened(project: Project, file: VirtualFile, serverStarter: LspServerSupportProvider.LspServerStarter) {
        if (file.fileType == TypstFileType) {
            // Check if tinymist executable exists and start the server
            // We'll use a simple descriptor for now, assuming 'tinymist' is on PATH
            serverStarter.ensureServerStarted(TinymistLspServerDescriptor(project))
        }
    }
}

// Describes how to start and configure the tinymist LSP server
private class TinymistLspServerDescriptor(project: Project) : ProjectWideLspServerDescriptor(project, "Tinymist") {

    // Important: Check if the file is actually under the project's content root.
    // This prevents starting the server for external files opened in the editor.
    override fun isSupportedFile(file: VirtualFile): Boolean = file.fileType == TypstFileType // && ProjectRootManager.getInstance(project).fileIndex.isInContent(file) -> Add this later if needed

    @Throws(ExecutionException::class)
    override fun createCommandLine(): GeneralCommandLine {
        // TODO: Add error handling if 'tinymist' is not found on PATH
        // TODO: Allow configuring the path to the tinymist executable
        // TODO: Add necessary arguments if tinymist requires any flags to start as LSP server
        return GeneralCommandLine("tinymist")
            // Add any necessary arguments like: .withParameters("lsp")
            // Add environment variables if needed: .withEnvironment(...)
            .withWorkDirectory(project.basePath) // Set working directory to project root
    }

    // We can override other methods here for more specific configurations,
    // like initialization options, custom messages, etc.
    // override fun getLanguageId(file: VirtualFile): String? = "typst" // If needed
} 