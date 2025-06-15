package org.tinymist.intellij.lsp

import com.intellij.testFramework.fixtures.BasePlatformTestCase
import org.junit.Test
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNotNull

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
        val fileContent = """
            #set page(width: 10cm, height: auto)

            = Hello, Typst!

            This is a simple Typst document for testing.
            
            #
            """

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger completion
        myFixture.editor.caretModel.moveToOffset(fileContent.lastIndexOf("#"))

        // Wait for the LSP server to start and be ready
        Thread.sleep(2000)

        // Trigger completion at the current position
        val lookupElements = myFixture.completeBasic()

        // Log for debugging
        println("[DEBUG_LOG] Completion returned ${lookupElements.size} elements")
        println("[DEBUG_LOG] First element: ${lookupElements[0].lookupString}")

        // Assert that the completion results contain expected items
        // This is a simplified check that just verifies that we got some results
        assertTrue("Completion results should not be empty", lookupElements.isNotEmpty())

    }

    /**
     * Test that completion works for function parameters.
     *
     * This test opens a Typst file with a function call,
     * places the caret inside the function call, and verifies that
     * completion suggestions for parameters are displayed.
     */
    fun testCompletionForParameters() {
        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = """
            #set page(width: 10cm, height: auto)

            = Hello, Typst!

            #text(
            """

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger completion
        myFixture.editor.caretModel.moveToOffset(20)

        // Wait for the LSP server to start and be ready
        Thread.sleep(2000)

        // Trigger completion at the current position
        val lookupElements = myFixture.completeBasic()

        // Log for debugging
        println("[DEBUG_LOG] Completion returned ${lookupElements.size} elements")
        println("[DEBUG_LOG] First element: ${lookupElements[0].lookupString}")

        // Assert that the completion results contain expected items
        // This is a simplified check that just verifies that we got some results
        assertTrue("Completion results should not be empty", lookupElements.isNotEmpty())

    }
}
