package org.tinymist.intellij.lsp

import com.intellij.lang.documentation.ide.IdeDocumentationTargetProvider
import com.intellij.platform.backend.documentation.DocumentationTarget
import com.intellij.platform.backend.documentation.impl.computeDocumentationBlocking
import com.intellij.util.containers.ContainerUtil

/**
 * Test for hover functionality in Typst files.
 * 
 * This test verifies that the hover functionality works correctly
 * by opening a Typst file and simulating a hover event.
 */
class TypstHoverTest : TypstPlatformTestCase() {

    /**
     * Test that hover works for a simple Typst file.
     * 
     * This test opens a Typst file with a simple function definition,
     * places the caret on a parameter, and simulates a hover event.
     */
    fun testHoverOnParameter() {
        configureTinymistExecutableForTests()

        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = """
            #let highlight(content) = {
              text(fill: red, content)
            }

            #highlight[This text should be highlighted in red.]
            """

        // Configure the test fixture with a real project file. LSP4IJ needs a
        // file-backed VirtualFile; light files throw from VirtualFile.toNioPath.
        myFixture.configureByPhysicalText(fileName, fileContent)

        // Move the caret to the position where we want to trigger hover
        val offset = fileContent.indexOf("content)")
        myFixture.editor.caretModel.moveToOffset(offset)

        waitForTinymistLanguageServerReady()

        // Get the documentation target at the caret position
        val targets = getDocumentationTargets()

        // Verify that we have at least one documentation target
        assertFalse("No documentation targets found", targets.isEmpty())

        // Get the HTML content of the documentation
        val html = getDocumentationHtml(targets.first())

        // Verify that the HTML content is not null or empty
        assertNotNull("Documentation HTML is null", html)
        assertFalse("Documentation HTML is empty", html?.isEmpty() ?: true)

        // Log the HTML content for debugging
        println("[DEBUG_LOG] Documentation HTML: $html")

        // Verify that the HTML content contains expected text
        assertTrue("Documentation HTML does not contain expected content", 
                  html?.contains("content") == true || html?.contains("parameter") == true)
    }

    /**
     * Test that hover works for a function call.
     */
    fun testHoverOnFunctionCall() {
        configureTinymistExecutableForTests()

        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = """
            #let highlight(content) = {
              text(fill: red, content)
            }

            #highlight[This text should be highlighted in red.]
            """

        // Configure the test fixture with a real project file. LSP4IJ needs a
        // file-backed VirtualFile; light files throw from VirtualFile.toNioPath.
        myFixture.configureByPhysicalText(fileName, fileContent)

        // Move the caret to the position where we want to trigger hover (on the function call)
        val offset = fileContent.indexOf("#highlight")
        myFixture.editor.caretModel.moveToOffset(offset + 1) // Position after the # character

        waitForTinymistLanguageServerReady()

        // Get the documentation target at the caret position
        val targets = getDocumentationTargets()

        // Verify that we have at least one documentation target
        assertFalse("No documentation targets found", targets.isEmpty())

        // Get the HTML content of the documentation
        val html = getDocumentationHtml(targets.first())

        // Verify that the HTML content is not null or empty
        assertNotNull("Documentation HTML is null", html)
        assertFalse("Documentation HTML is empty", html?.isEmpty() ?: true)

        // Log the HTML content for debugging
        println("[DEBUG_LOG] Documentation HTML: $html")

        // Verify that the HTML content contains expected text
        assertTrue("Documentation HTML does not contain expected content", 
                  html?.contains("highlight") == true || html?.contains("function") == true)
    }

    /**
     * Helper method to get documentation targets at the current caret position.
     * @return List of DocumentationTarget objects
     */
    private fun getDocumentationTargets(): List<DocumentationTarget> {
        val editor = myFixture.editor
        val file = myFixture.file
        val offset = editor.caretModel.offset

        val targets = mutableListOf<DocumentationTarget>()
        ContainerUtil.addAllNotNull(
            targets, 
            IdeDocumentationTargetProvider.getInstance(project).documentationTargets(editor, file, offset)
        )

        return targets
    }

    /**
     * Helper method to get the HTML content of a documentation target.
     * @param target The DocumentationTarget to get HTML content for
     * @return The HTML content as a String, or null if no documentation is available
     */
    private fun getDocumentationHtml(target: DocumentationTarget): String? {
        return computeDocumentationBlocking(target.createPointer())?.html
    }
}
