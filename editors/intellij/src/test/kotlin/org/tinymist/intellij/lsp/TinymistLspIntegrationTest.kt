package org.tinymist.intellij.lsp

import com.intellij.openapi.command.WriteCommandAction
import com.intellij.openapi.fileEditor.FileDocumentManager
import org.tinymist.intellij.TypstFileType

class TinymistLspIntegrationTest : TypstPlatformTestCase() {

    /**
     * Test that verifies the LSP server is correctly started when a Typst file is opened.
     * This test requires a Tinymist executable to be available.
     */
    fun testLspServerStartsForTypstFile() {
        configureTinymistExecutableForTests()

        // Creates a temporary Typst file
        val fileName = "test.typ"
        val fileContent = "#set page(width: 10cm, height: auto)\n\n= Hello, Typst!\n\nThis is a test document."

        // Configures the test fixture with a real project file. LSP4IJ needs a
        // file-backed VirtualFile; light files throw from VirtualFile.toNioPath.
        myFixture.configureByPhysicalText(fileName, fileContent)

        // Gets the virtual file
        val virtualFile = myFixture.file.virtualFile

        // Verifies that the file is recognized as a Typst file
        assertEquals(TypstFileType, virtualFile.fileType)

        // Gets the document for the file
        val document = FileDocumentManager.getInstance().getDocument(virtualFile)
        assertNotNull("Document should not be null", document)

        // Triggers LSP initialization by making a change to the document
        WriteCommandAction.runWriteCommandAction(project) {
            document!!.insertString(document.textLength, "\n\nAdded text for testing.")
        }

        val server = waitForTinymistLanguageServerReady()
        assertEquals(TINYMIST_SERVER_ID, server.serverDefinition.id)
    }

    /**
     * Test that verifies basic LSP features like code completion work correctly.
     * This test requires the actual tinymist executable to be available.
     */
    fun testLspCompletion() {
        configureTinymistExecutableForTests()

        // Creates a temporary Typst file with content that should trigger completion
        val fileName = "completion_test.typ"
        val fileContent = "#set page(width: 10cm, height: auto)\n\n#"

        // Configures the test fixture with a real project file. LSP4IJ needs a
        // file-backed VirtualFile; light files throw from VirtualFile.toNioPath.
        myFixture.configureByPhysicalText(fileName, fileContent)

        // Moves the caret to the position where we want to trigger completion
        myFixture.editor.caretModel.moveToOffset(fileContent.length)

        waitForTinymistLanguageServerReady()

        // Triggers completion at the current position
        val lookupElements = myFixture.completeBasic()

        // Verifies that we got some completion results
        assertNotNull("Completion should return lookup elements", lookupElements)
        assertTrue("Completion should return at least one result", lookupElements.isNotEmpty())

        // Verifies that common Typst functions are included in the completion results
        val completionTexts = lookupElements.map { it.lookupString }
        assertTrue("Completion should include 'text' function", completionTexts.contains("text"))
    }

}
