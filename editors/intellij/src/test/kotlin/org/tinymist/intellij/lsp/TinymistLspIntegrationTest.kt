package org.tinymist.intellij.lsp

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.command.WriteCommandAction
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import org.junit.Test
import org.tinymist.intellij.TypstFileType

class TinymistLspIntegrationTest : BasePlatformTestCase() {

    /**
     * Test that verifies the LSP server is correctly started when a Typst file is opened.
     * This test uses a mock LSP server to avoid dependencies on the actual tinymist executable.
     */
    @Test
    fun testLspServerStartsForTypstFile() {
        // Create a temporary Typst file
        val fileName = "test.typ"
        val fileContent = "#set page(width: 10cm, height: auto)\n\n= Hello, Typst!\n\nThis is a test document."

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Get the virtual file
        val virtualFile = myFixture.file.virtualFile

        // Verify that the file is recognized as a Typst file
        assertEquals(TypstFileType, virtualFile.fileType)

        // Wait for the LSP server to start (this is a simplified approach)
        // In a real test, you would need to wait for the server to be ready

        // Get the document for the file
        val document = FileDocumentManager.getInstance().getDocument(virtualFile)
        assertNotNull("Document should not be null", document)

        // Trigger LSP initialization by making a change to the document
        WriteCommandAction.runWriteCommandAction(project) {
            document!!.insertString(document.textLength, "\n\nAdded text for testing.")
        }

        // Verify that the LSP server exists for this file
        // Note: This is a simplified check. In a real test, you would need to
        // verify that the server is actually running and responding to requests.
        val languageServiceAccessor = com.redhat.devtools.lsp4ij.LanguageServiceAccessor.getInstance(project)
        val hasServer = languageServiceAccessor.hasAny(myFixture.file) { true }
        assertTrue("LSP server should exist for Typst files", hasServer)
    }

    /**
     * Test that verifies basic LSP features like code completion work correctly.
     * This test requires the actual tinymist executable to be available.
     */
    @Test
    fun testLspCompletion() {
        // Create a temporary Typst file with content that should trigger completion
        val fileName = "completion_test.typ"
        val fileContent = "#set page(width: 10cm, height: auto)\n\n#"

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger completion
        myFixture.editor.caretModel.moveToOffset(fileContent.length)

        // Wait for the LSP server to start and be ready
        waitForLspServerReady()

        // Trigger completion at the current position
        val lookupElements = myFixture.completeBasic()

        // Verify that we got some completion results
        assertNotNull("Completion should return lookup elements", lookupElements)
        assertTrue("Completion should return at least one result", lookupElements.isNotEmpty())

        // Verify that common Typst functions are included in the completion results
        val completionTexts = lookupElements.map { it.lookupString }
        assertTrue("Completion should include 'text' function", completionTexts.contains("text"))
    }

    /**
     * Helper method to wait for the LSP server to be ready.
     * This is a simplified approach and might need to be adjusted based on the actual behavior.
     */
    private fun waitForLspServerReady() {
        // Wait for a reasonable amount of time for the server to start
        Thread.sleep(2000)
    }
}
