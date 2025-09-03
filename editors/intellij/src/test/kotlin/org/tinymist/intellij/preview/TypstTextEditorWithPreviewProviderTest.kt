package org.tinymist.intellij.preview

import com.intellij.testFramework.fixtures.BasePlatformTestCase

class TypstTextEditorWithPreviewProviderTest : BasePlatformTestCase() {

    private lateinit var provider: TypstTextEditorWithPreviewProvider

    override fun setUp() {
        super.setUp()
        provider = TypstTextEditorWithPreviewProvider()
    }

    fun testAcceptTypstFile() {
        // Create a temporary Typst file
        val fileName = "test.typ"
        myFixture.configureByText(fileName, "")
        
        // Get the virtual file
        val virtualFile = myFixture.file.virtualFile
        
        // Verify that the provider accepts the Typst file
        assertTrue("Provider should accept Typst files", provider.accept(project, virtualFile))
    }

    fun testRejectNonTypstFile() {
        // Create a temporary non-Typst file
        val fileName = "test.txt"
        myFixture.configureByText(fileName, "")
        
        // Get the virtual file
        val virtualFile = myFixture.file.virtualFile
        
        // Verify that the provider rejects the non-Typst file
        assertFalse("Provider should reject non-Typst files", provider.accept(project, virtualFile))
    }
}