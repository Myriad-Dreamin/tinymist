package org.tinymist.intellij.lsp

import com.intellij.openapi.progress.ProcessCanceledException
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.util.SystemInfo
import com.redhat.devtools.lsp4ij.installation.LanguageServerInstallerBase
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.jetbrains.annotations.NotNull
import org.tinymist.intellij.settings.TinymistVersion
import java.io.FileOutputStream
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.StandardCopyOption
import java.util.zip.GZIPInputStream
import java.util.zip.ZipInputStream
import com.intellij.openapi.application.PathManager

/**
 * Installer for the Tinymist language server.
 * 
 * Downloads and installs the Tinymist binary based on the current platform.
 * Uses the correct GitHub asset names from the actual releases.
 */
class TinymistLanguageServerInstaller : LanguageServerInstallerBase() {
    
    companion object {
        private const val GITHUB_RELEASES_URL = "https://github.com/Myriad-Dreamin/tinymist/releases/download"
        private const val EXECUTABLE_NAME = "tinymist"
        private const val WINDOWS_EXECUTABLE_NAME = "tinymist.exe"
        
        // Platform-specific download URLs and archive names (using actual GitHub release names)
        private val PLATFORM_INFO = when {
            SystemInfo.isWindows && isX64Architecture() -> PlatformInfo(
                "tinymist-x86_64-pc-windows-msvc.zip", 
                WINDOWS_EXECUTABLE_NAME,
                ArchiveType.ZIP
            )
            SystemInfo.isMac && isAarch64Architecture() -> PlatformInfo(
                "tinymist-aarch64-apple-darwin.tar.gz", 
                EXECUTABLE_NAME,
                ArchiveType.TAR_GZ
            )
            SystemInfo.isMac && isX64Architecture() -> PlatformInfo(
                "tinymist-x86_64-apple-darwin.tar.gz", 
                EXECUTABLE_NAME,
                ArchiveType.TAR_GZ
            )
            SystemInfo.isLinux && isX64Architecture() -> PlatformInfo(
                "tinymist-x86_64-unknown-linux-gnu.tar.gz", 
                EXECUTABLE_NAME,
                ArchiveType.TAR_GZ
            )
            SystemInfo.isLinux && isAarch64Architecture() -> PlatformInfo(
                "tinymist-aarch64-unknown-linux-gnu.tar.gz", 
                EXECUTABLE_NAME,
                ArchiveType.TAR_GZ
            )
            else -> null
        }
        
        private fun isX64Architecture(): Boolean {
            val arch = System.getProperty("os.arch", "").lowercase()
            return arch.contains("x86_64") || arch.contains("amd64")
        }
        
        private fun isAarch64Architecture(): Boolean {
            val arch = System.getProperty("os.arch", "").lowercase()
            return arch.contains("aarch64") || arch.contains("arm64")
        }
    }
    
    private enum class ArchiveType {
        ZIP, TAR_GZ
    }
    
    private data class PlatformInfo(
        val archiveName: String,
        val executableName: String,
        val archiveType: ArchiveType
    )
    
    /**
     * Gets the directory where Tinymist should be installed.
     * Uses the plugin data directory under the user's home.
     */
    private fun getInstallationDir(): Path {
        val pluginDir = Paths.get(PathManager.PROPERTY_SYSTEM_PATH, "tinymist-intellij")
        return pluginDir.resolve("server").resolve(TinymistVersion.CURRENT)
    }
    
    /**
     * Gets the path to the installed Tinymist executable.
     */
    private fun getExecutablePath(): Path? {
        val platformInfo = PLATFORM_INFO ?: return null
        return getInstallationDir().resolve(platformInfo.executableName)
    }
    
    /**
     * Checks if the Tinymist server is installed and executable.
     */
    override fun checkServerInstalled(@NotNull indicator: ProgressIndicator): Boolean {
        progress("Checking if Tinymist server is installed...", indicator)
        ProgressManager.checkCanceled()
        
        val executablePath = getExecutablePath()
        if (executablePath == null) {
            progress("Platform not supported for Tinymist installation", indicator)
            return false
        }
        
        val isInstalled = Files.exists(executablePath) && Files.isExecutable(executablePath)
        
        if (isInstalled) {
            progress("Tinymist server found at: $executablePath", indicator)
        } else {
            progress("Tinymist server not found or not executable", indicator)
        }
        
        return isInstalled
    }
    
    /**
     * Downloads and installs the Tinymist server binary.
     */
    override fun install(indicator: ProgressIndicator) {
        val platformInfo = PLATFORM_INFO 
            ?: throw UnsupportedOperationException("Tinymist installation is not supported on this platform")
            
        val installationDir = getInstallationDir()
        val downloadUrl = "$GITHUB_RELEASES_URL/${TinymistVersion.CURRENT}/${platformInfo.archiveName}"
        
        try {
            // Step 1: Create installation directory
            progress("Creating installation directory...", 0.1, indicator)
            ProgressManager.checkCanceled()
            Files.createDirectories(installationDir)
            
            // Step 2: Download the archive
            progress("Downloading Tinymist ${TinymistVersion.CURRENT} for ${getCurrentPlatformName()}...", 0.2, indicator)
            ProgressManager.checkCanceled()
            val tempArchive = Files.createTempFile("tinymist", getArchiveExtension(platformInfo.archiveType))
            
            try {
                downloadFile(downloadUrl, tempArchive)
                
                // Step 3: Extract the archive
                progress("Extracting Tinymist archive...", 0.6, indicator)
                ProgressManager.checkCanceled()
                extractArchive(tempArchive, installationDir, platformInfo)
                
                // Step 4: Set executable permissions (Unix-like systems)
                progress("Setting up executable permissions...", 0.8, indicator)
                ProgressManager.checkCanceled()
                val executablePath = getExecutablePath()
                    ?: throw RuntimeException("Failed to get executable path after platform validation")
                if (!SystemInfo.isWindows) {
                    executablePath.toFile().setExecutable(true, false)
                }
                
                // Step 5: Verify installation
                progress("Verifying installation...", 0.9, indicator)
                ProgressManager.checkCanceled()
                if (!Files.exists(executablePath) || !Files.isExecutable(executablePath)) {
                    throw RuntimeException("Failed to install Tinymist: executable not found or not executable")
                }
                
                progress("Tinymist installation completed successfully!", 1.0, indicator)
                
            } finally {
                // Clean up temporary file
                Files.deleteIfExists(tempArchive)
            }
            
        } catch (e: ProcessCanceledException) {
            // Re-throw ProcessCanceledException to properly handle cancellation
            throw e
        } catch (e: Exception) {
            throw RuntimeException("Failed to install Tinymist language server: ${e.message}", e)
        }
    }
    
    /**
     * Downloads a file from the given URL to the specified path.
     */
    private fun downloadFile(url: String, destination: Path) {
        val client = HttpClient.newBuilder()
            .followRedirects(HttpClient.Redirect.ALWAYS)
            .connectTimeout(java.time.Duration.ofSeconds(30))
            .build()
            
        val request = HttpRequest.newBuilder()
            .uri(URI.create(url))
            .timeout(java.time.Duration.ofMinutes(5))
            .build()
            
        val response = client.send(request, HttpResponse.BodyHandlers.ofInputStream())
        
        if (response.statusCode() != 200) {
            throw RuntimeException("Failed to download from $url: HTTP ${response.statusCode()}")
        }
        
        response.body().use { inputStream ->
            Files.copy(inputStream, destination, StandardCopyOption.REPLACE_EXISTING)
        }
    }
    
    /**
     * Extracts the downloaded archive and places the executable in the installation directory.
     */
    private fun extractArchive(archivePath: Path, installationDir: Path, platformInfo: PlatformInfo) {
        when (platformInfo.archiveType) {
            ArchiveType.ZIP -> extractZip(archivePath, installationDir, platformInfo.executableName)
            ArchiveType.TAR_GZ -> extractTarGz(archivePath, installationDir, platformInfo.executableName)
        }
    }
    
    private fun extractZip(archivePath: Path, installationDir: Path, executableName: String) {
        ZipInputStream(Files.newInputStream(archivePath)).use { zipStream ->
            var entry = zipStream.nextEntry
            while (entry != null) {
                ProgressManager.checkCanceled()
                
                // Look for the tinymist executable in the archive
                if (entry.name.endsWith(executableName) && !entry.isDirectory) {
                    val executablePath = installationDir.resolve(executableName)
                    FileOutputStream(executablePath.toFile()).use { outputStream ->
                        zipStream.copyTo(outputStream)
                    }
                    break
                }
                entry = zipStream.nextEntry
            }
        }
    }
    
    private fun extractTarGz(archivePath: Path, installationDir: Path, executableName: String) {
        GZIPInputStream(Files.newInputStream(archivePath)).use { gzipStream ->
            TarArchiveInputStream(gzipStream).use { tarStream ->
                var entry = tarStream.nextEntry
                while (entry != null) {
                    ProgressManager.checkCanceled()
                    
                    // Look for the tinymist executable in the archive
                    if (entry.name.endsWith(executableName) && !entry.isDirectory) {
                        val executablePath = installationDir.resolve(executableName)
                        FileOutputStream(executablePath.toFile()).use { outputStream ->
                            tarStream.copyTo(outputStream)
                        }
                        break
                    }
                    entry = tarStream.nextEntry
                }
            }
        }
    }
    
    private fun getArchiveExtension(archiveType: ArchiveType): String {
        return when (archiveType) {
            ArchiveType.ZIP -> ".zip"
            ArchiveType.TAR_GZ -> ".tar.gz"
        }
    }
    
    /**
     * Gets a human-readable name for the current platform.
     */
    private fun getCurrentPlatformName(): String {
        return when {
            SystemInfo.isWindows -> "Windows"
            SystemInfo.isMac -> "macOS"
            SystemInfo.isLinux -> "Linux"
            else -> "Unknown"
        }
    }
    
    /**
     * Gets the path to the installed Tinymist executable, or null if not installed.
     */
    fun getInstalledExecutablePath(): String? {
        val executablePath = getExecutablePath() ?: return null
        return if (Files.exists(executablePath) && Files.isExecutable(executablePath)) {
            executablePath.toString()
        } else {
            null
        }
    }
}