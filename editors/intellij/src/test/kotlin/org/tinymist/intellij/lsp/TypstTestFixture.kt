package org.tinymist.intellij.lsp

import com.intellij.openapi.util.Disposer
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.testFramework.PlatformTestUtil
import com.intellij.testFramework.common.ThreadLeakTracker
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.testFramework.fixtures.CodeInsightTestFixture
import com.intellij.testFramework.fixtures.IdeaTestFixtureFactory
import com.redhat.devtools.lsp4ij.LanguageServerItem
import com.redhat.devtools.lsp4ij.LanguageServiceAccessor
import org.tinymist.intellij.settings.ServerManagementMode
import org.tinymist.intellij.settings.TinymistSettingsService
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.TimeUnit

internal const val TINYMIST_SERVER_ID = "tinymistServer"

abstract class TypstPlatformTestCase : BasePlatformTestCase() {
    override fun setUp() {
        registerThreadLeakWhitelist()
        super.setUp()
    }

    override fun createMyFixture(): CodeInsightTestFixture {
        val fixtureFactory = IdeaTestFixtureFactory.getFixtureFactory()
        // LSP4IJ maps the guessed project dir to a NIO path; light fixtures use temp:///src.
        val projectFixture = fixtureFactory
            .createFixtureBuilder(getTestName(false), true)
            .fixture
        return fixtureFactory.createCodeInsightFixture(
            projectFixture,
            fixtureFactory.createTempDirTestFixture(),
        )
    }

    protected fun waitForTinymistLanguageServerReady(
        timeoutMillis: Int = 30_000,
    ): LanguageServerItem {
        val future = LanguageServiceAccessor.getInstance(project)
            .getLanguageServers(myFixture.file, { true }, { true })

        PlatformTestUtil.waitWithEventsDispatching(
            "Timed out waiting for Tinymist language server discovery",
            { future.isDone },
            timeoutMillis,
        )

        val item = future.get()
            .firstOrNull { it.serverDefinition.id == TINYMIST_SERVER_ID }
            ?: throw AssertionError("Tinymist language server was not discovered for ${myFixture.file.virtualFile.path}")

        val initializedServer = item.initializedServer
        PlatformTestUtil.waitWithEventsDispatching(
            "Timed out waiting for Tinymist language server initialization",
            { initializedServer.isDone },
            timeoutMillis,
        )
        initializedServer.get()

        return item
    }

    private companion object {
        private val threadLeakWhitelistDisposable =
            Disposer.newDisposable("Tinymist IntelliJ LSP test thread whitelist")
        private var isThreadLeakWhitelistRegistered = false

        private fun registerThreadLeakWhitelist() {
            synchronized(threadLeakWhitelistDisposable) {
                if (!isThreadLeakWhitelistRegistered) {
                    ThreadLeakTracker.longRunningThreadCreated(threadLeakWhitelistDisposable, "SystemPropertyWatcher")
                    isThreadLeakWhitelistRegistered = true
                }
            }
        }
    }
}

internal fun CodeInsightTestFixture.configureByPhysicalText(
    fileName: String,
    fileContent: String,
): VirtualFile {
    val file = tempDirFixture.createFile(fileName, fileContent)
    configureFromExistingVirtualFile(file)
    return file
}

internal fun configureTinymistExecutableForTests() {
    val executablePath = findTinymistExecutable()

    val settingsService = TinymistSettingsService.instance
    settingsService.serverManagementMode = ServerManagementMode.CUSTOM_PATH
    settingsService.tinymistExecutablePath = executablePath
}

private fun findTinymistExecutable(): String {
    val diagnostics = mutableListOf<String>()

    for ((label, command) in tinymistExecutableCandidates()) {
        when (val result = probeTinymistExecutable(command)) {
            is ProbeResult.Available -> return result.command
            is ProbeResult.Unavailable -> diagnostics += "$label ('$command'): ${result.reason}"
        }
    }

    throw AssertionError(
        "Could not find a valid tinymist executable for IntelliJ LSP tests.\n" +
            diagnostics.joinToString(separator = "\n") +
            "\nBuild one with `cargo build --bin tinymist` or set TINYMIST_EXECUTABLE.",
    )
}

private fun tinymistExecutableCandidates(): List<Pair<String, String>> {
    val binaryName = if (isWindows()) "tinymist.exe" else "tinymist"
    val root = findGitRoot()
    val candidates = buildList {
        System.getenv("TINYMIST_EXECUTABLE")
            ?.takeIf(String::isNotBlank)
            ?.let { add("TINYMIST_EXECUTABLE" to it) }
        add("Bundled VSCode" to root.resolve("editors/vscode/out/$binaryName").toString())
        add("Cargo debug" to root.resolve("target/debug/$binaryName").toString())
        add("Cargo release" to root.resolve("target/release/$binaryName").toString())
        TinymistLanguageServerInstaller().getInstalledExecutablePath()
            ?.let { add("Installed by LSP4IJ" to it) }
        add("In PATH" to binaryName)
    }

    return candidates.distinctBy { it.second }
}

private fun findGitRoot(): Path {
    var directory = Path.of("").toAbsolutePath().normalize()

    while (directory.parent != null) {
        if (Files.exists(directory.resolve(".git"))) {
            return directory
        }
        directory = directory.parent
    }

    return Path.of("").toAbsolutePath().normalize()
}

private fun isExecutableFile(path: String): Boolean {
    return runCatching {
        val executablePath = Path.of(path)
        Files.isRegularFile(executablePath) && Files.isExecutable(executablePath)
    }.getOrDefault(false)
}

private fun isPathLike(command: String): Boolean {
    return command.contains('/') || command.contains('\\') || Path.of(command).isAbsolute
}

private fun isWindows(): Boolean {
    return System.getProperty("os.name").lowercase().contains("win")
}

private fun probeTinymistExecutable(command: String): ProbeResult {
    val executablePath = resolveExecutablePathForSettings(command)
    if (executablePath == null && isPathLike(command)) {
        return ProbeResult.Unavailable("not an executable file")
    }

    return try {
        val process = ProcessBuilder(command, "probe")
            .redirectErrorStream(true)
            .start()
        val finished = process.waitFor(PROBE_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        val output = process.inputStream.bufferedReader().readText().trim()

        if (!finished) {
            process.destroyForcibly()
            return ProbeResult.Unavailable("timed out while running `tinymist probe`")
        }

        if (process.exitValue() == 0) {
            executablePath
                ?.let(ProbeResult::Available)
                ?: ProbeResult.Unavailable("probe succeeded but the command could not be resolved to an executable path")
        } else {
            ProbeResult.Unavailable(
                "probe exited with ${process.exitValue()}" +
                    output.takeIf(String::isNotBlank)?.let { ": $it" }.orEmpty(),
            )
        }
    } catch (error: IOException) {
        ProbeResult.Unavailable(error.message ?: error::class.java.simpleName)
    }
}

private fun resolveExecutablePathForSettings(command: String): String? {
    if (isPathLike(command)) {
        return command.takeIf(::isExecutableFile)
    }

    return findExecutableInPath(command)?.toString()
}

private fun findExecutableInPath(binaryName: String): Path? {
    val path = System.getenv("PATH") ?: return null

    return path.split(File.pathSeparator)
        .asSequence()
        .filter(String::isNotBlank)
        .map { Path.of(it).resolve(binaryName).toAbsolutePath().normalize() }
        .firstOrNull { Files.isRegularFile(it) && Files.isExecutable(it) }
}

private sealed class ProbeResult {
    data class Available(val command: String) : ProbeResult()
    data class Unavailable(val reason: String) : ProbeResult()
}

private const val PROBE_TIMEOUT_SECONDS = 10L
