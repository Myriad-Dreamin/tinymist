package org.tinymist.intellij.lsp

import com.intellij.lang.documentation.ide.IdeDocumentationTargetProvider
import com.intellij.platform.backend.documentation.DocumentationTarget
import com.intellij.platform.backend.documentation.impl.computeDocumentationBlocking
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.util.containers.ContainerUtil
import org.junit.Test
import java.awt.event.InputEvent
import java.awt.event.MouseEvent

/**
 * Test for hover functionality in Typst files.
 * 
 * This test verifies that the hover functionality works correctly
 * by opening a Typst file and simulating a hover event.
 */
class TypstHoverTest : BasePlatformTestCase() {

    /**
     * Test that hover works for a simple Typst file.
     * 
     * This test opens a Typst file with a simple function definition,
     * places the caret on a parameter, and simulates a hover event.
     */
    @Test
    fun testHoverOnParameter() {
        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = """
            #let highlight(content) = {
              text(fill: red, content)
            }

            #highlight[This text should be highlighted in red.]
            """

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger hover
        val offset = fileContent.indexOf("content)")
        myFixture.editor.caretModel.moveToOffset(offset)

        // Wait for the LSP server to start and be ready
        Thread.sleep(2000)

        // Simulate a mouse hover event at the current caret position
        simulateMouseHover()

        // Wait for the hover tooltip to appear
        Thread.sleep(500)

        // Log for debugging
        println("[DEBUG_LOG] Hover event simulated at caret position for parameter")

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
    @Test
    fun testHoverOnFunctionCall() {
        // Create a temporary Typst file with content
        val fileName = "test.typ"
        val fileContent = """
            #let highlight(content) = {
              text(fill: red, content)
            }

            #highlight[This text should be highlighted in red.]
            """

        // Configure the test fixture with the file
        myFixture.configureByText(fileName, fileContent)

        // Move the caret to the position where we want to trigger hover (on the function call)
        val offset = fileContent.indexOf("#highlight")
        myFixture.editor.caretModel.moveToOffset(offset + 1) // Position after the # character

        // Wait for the LSP server to start and be ready
        Thread.sleep(2000)

        // Simulate a mouse hover event at the current caret position
        simulateMouseHover()

        // Wait for the hover tooltip to appear
        Thread.sleep(500)

        // Log for debugging
        println("[DEBUG_LOG] Hover event simulated at caret position for function call")

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
     * Helper method to simulate a mouse hover event at the current caret position.
     * This triggers the hover tooltip to appear.
     */
    private fun simulateMouseHover() {
        val editor = myFixture.editor
        val editorComponent = editor.contentComponent
        val point = editor.visualPositionToXY(editor.caretModel.visualPosition)

        // Create a mouse event that simulates hovering
        val event = MouseEvent(
            editorComponent,
            MouseEvent.MOUSE_MOVED,
            System.currentTimeMillis(),
            InputEvent.BUTTON1_DOWN_MASK,
            point.x,
            point.y,
            1,
            false
        )

        // Dispatch the event to the editor component
        editorComponent.dispatchEvent(event)
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
