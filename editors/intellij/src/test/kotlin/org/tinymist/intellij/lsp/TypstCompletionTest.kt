package org.tinymist.intellij.lsp

import com.intellij.testFramework.fixtures.BasePlatformTestCase

/**
 * Test for completion functionality in Typst files.
 *
 * This test verifies that the completion functionality works correctly
 * by opening a Typst file and checking that completion suggestions are displayed.
 */
class TypstCompletionTest : BasePlatformTestCase() {

    /**
     * Test that completion works for a simple Typst file.
     *
     * This test opens a Typst file with a simple function call,
     * places the caret after the # character, and verifies that
     * completion suggestions are displayed when the completion action is triggered.
     */
    fun testCompletionAfterHash() {
        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = "#"

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger completion
        myFixture.editor.caretModel.moveToOffset(1)

        // Wait for the LSP server to start and be ready
        Thread.sleep(2000)

        // Trigger completion at the current position
        val lookupElements = myFixture.completeBasic()

        // Assert that the completion results contain expected items
        // This is a simplified check that just verifies that we got some results
        assertTrue("Completion results should not be empty", lookupElements.isNotEmpty())

        // Log for debugging
        println("[DEBUG_LOG] Completion returned ${lookupElements.size} elements")
        println("[DEBUG_LOG] First element: ${lookupElements[0].lookupString}")
    }
}
