package org.tinymist.intellij.lsp

import com.intellij.openapi.vfs.VirtualFile
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.testFramework.fixtures.CodeInsightTestFixture
import com.intellij.testFramework.fixtures.IdeaTestFixtureFactory
import org.tinymist.intellij.settings.ServerManagementMode
import org.tinymist.intellij.settings.TinymistSettingsService
import java.nio.file.Files
import java.nio.file.Path

abstract class TypstPlatformTestCase : BasePlatformTestCase() {
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
}

internal fun CodeInsightTestFixture.configureByPhysicalText(
    fileName: String,
    fileContent: String,
): VirtualFile {
    val file = tempDirFixture.createFile(fileName, fileContent)
    configureFromExistingVirtualFile(file)
    return file
}

internal fun configureTinymistExecutableForTests(): Boolean {
    val executablePath = findTinymistExecutable() ?: return false

    val settingsService = TinymistSettingsService.instance
    settingsService.serverManagementMode = ServerManagementMode.CUSTOM_PATH
    settingsService.tinymistExecutablePath = executablePath
    return true
}

private fun findTinymistExecutable(): String? {
    return System.getenv("TINYMIST_EXECUTABLE")
        ?.takeIf(::isExecutableFile)
        ?: TinymistLanguageServerInstaller().getInstalledExecutablePath()
}

private fun isExecutableFile(path: String): Boolean {
    return runCatching {
        val executablePath = Path.of(path)
        Files.isRegularFile(executablePath) && Files.isExecutable(executablePath)
    }.getOrDefault(false)
}
