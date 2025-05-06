package org.tinymist.intellij.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.ProjectLocator
import com.intellij.openapi.vfs.VirtualFile
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import org.eclipse.lsp4j.services.LanguageServer
import java.nio.file.Files
import java.nio.file.Paths

// Define a data class for initialization options mirroring VSCode's config structure
// TODO: Populate this with actual configurable settings from IntelliJ's settings system
data class TinymistInitializationOptions(
    val serverPath: String? = null, // Example setting
    // Add other relevant fields based on VSCode config and tinymist server needs
    // e.g., fontPaths, exportPdf, preview settings, etc.
    val lspInputs: Map<String, String> = mapOf("x-preview" to "{\"version\":1,\"theme\":\"\"}") // Placeholder from logs
)

class TinymistLspStreamConnectionProvider(private val project: Project) : ProcessStreamConnectionProvider() {

    init {
        val executable = findTinymistExecutable()
            ?: throw RuntimeException("Tinymist executable not found on PATH during initialization. Please configure the path or ensure it's on PATH.")
        super.setCommands(mutableListOf(executable, "lsp"))
        // Consider setting working directory here if it's static:
        // super.setWorkingDirectory(project.basePath) 
    }

    private fun findTinymistExecutable(): String? {
        val name = if (System.getProperty("os.name").startsWith("Windows")) "tinymist.exe" else "tinymist"
        // TODO: Prioritize settings path if available
        val pathVar = System.getenv("PATH") ?: return null
        val paths = pathVar.split(System.getProperty("path.separator"))
        for (dir in paths) {
            val file = Paths.get(dir, name)
            if (Files.exists(file) && Files.isExecutable(file)) {
                return file.toString()
            }
        }
        // TODO: Also check bundled location if distributing with the plugin
        return null
    }

    // getCommands() is no longer overridden as commands are set in init via super.setCommands()
    // override fun getCommands(): MutableList<String> {
    //     // DIAGNOSTIC: Temporarily return a harmless command to avoid exceptions during LSP registry initialization
    //     // val executable = findTinymistExecutable()
    //     //    ?: throw RuntimeException("Tinymist executable not found. Please configure the path or ensure it's on PATH.")
    //     // return mutableListOf(executable, "lsp")
    //     return mutableListOf("echo", "LSP server placeholder") // Simple command that should not throw an error
    // }

    // getWorkingDirectory() can still be overridden if dynamic, or set in init via super.setWorkingDirectory()
    // For now, let's keep it overridden if ProcessStreamConnectionProvider has it as abstract or open.
    // If an explicit working directory is needed and is static (e.g. project root), set it in init.
    // Otherwise, if null is acceptable, ProcessStreamConnectionProvider might default to project root or let server decide.
    override fun getWorkingDirectory(): String? {
         return project.basePath // Example: Use project base path, now project is non-null
        // return null 
    }

    override fun getInitializationOptions(virtualFile: VirtualFile): Any? {
        // project is now available from constructor
        // val projectFromLocator: Project? = ProjectLocator.getInstance().guessProjectForFile(virtualFile)
        
        // It's better to use the executable path found during init, if possible,
        // or re-find it if it can change per file (unlikely for serverPath).
        // For now, let's assume findTinymistExecutable() is cheap enough to call again or we store it.
        val executablePath = findTinymistExecutable() // Or retrieve from a field if stored in init
        return TinymistInitializationOptions(serverPath = executablePath)
    }

    fun getProvidedInterface(): Class<out LanguageServer> {
        return LanguageServer::class.java
    }
} 