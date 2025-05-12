package org.tinymist.intellij.structure

// Ensure this import path to TinymistOutlineItem is correct based on your project structure
import org.tinymist.intellij.lsp.TinymistOutlineItem
import com.intellij.openapi.diagnostic.Logger
import org.eclipse.lsp4j.Range
import org.eclipse.lsp4j.Position

object OutlineDataHolder {
    private val LOG = Logger.getInstance(OutlineDataHolder::class.java)
    private val outlineCache = mutableMapOf<String, List<TinymistOutlineItem>>()

    // Helper to create a mock Range
    private fun mockRange(startLine: Int, startChar: Int, endLine: Int, endChar: Int): Range {
        return Range(Position(startLine, startChar), Position(endLine, endChar))
    }

    private fun createMockOutlineItems(): List<TinymistOutlineItem> {
        LOG.info("OutlineDataHolder: Providing MOCK outline data.")
        return listOf(
            TinymistOutlineItem(
                name = "Mock Heading 1",
                detail = "Level 1",
                range = mockRange(0, 0, 0, 10),
                selectionRange = mockRange(0, 0, 0, 10),
                children = listOf(
                    TinymistOutlineItem(
                        name = "Mock Sub-item 1.1",
                        detail = "Level 2",
                        range = mockRange(1, 4, 1, 20),
                        selectionRange = mockRange(1, 4, 1, 20)
                    ),
                    TinymistOutlineItem(
                        name = "Mock Sub-item 1.2",
                        detail = "Level 2",
                        range = mockRange(2, 4, 2, 20),
                        selectionRange = mockRange(2, 4, 2, 20)
                    )
                )
            ),
            TinymistOutlineItem(
                name = "Mock Heading 2",
                detail = "Level 1",
                range = mockRange(3, 0, 3, 10),
                selectionRange = mockRange(3, 0, 3, 10)
            )
        )
    }

    fun getOutline(filePath: String): List<TinymistOutlineItem> {
        LOG.debug("OutlineDataHolder: Get outline for $filePath")
        // Return cached items, or mock data if no cache exists for the path.
        return outlineCache[filePath] ?: createMockOutlineItems()
    }

    fun updateOutline(filePath: String, items: List<TinymistOutlineItem>) {
        outlineCache[filePath] = items
        LOG.info("OutlineDataHolder: Updated outline for $filePath. Items: ${items.size}")
        // TODO: Trigger UI refresh for the structure view if not automatic.
        // This might involve using IntelliJ's MessageBus or other mechanisms
        // to notify the TypstStructureViewModel that its data has changed.
        // For example, you might get the Project associated with the filePath
        // and use project.messageBus.syncPublisher(SOME_TOPIC).outlineUpdated(filePath)
    }
}
