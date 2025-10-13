package org.tinymist.intellij

import com.intellij.testFramework.fixtures.BasePlatformTestCase

class TypstFileTypeTest : BasePlatformTestCase() {
    fun testFileTypeProperties() {
        // Test basic properties of the TypstFileType
        assertEquals("Typst file", TypstFileType.getName())
        assertEquals("Typst language file", TypstFileType.getDescription())
        assertEquals("typ", TypstFileType.getDefaultExtension())
        // Icon is currently null, so we just verify that
        assertNull(TypstFileType.getIcon())
    }

    fun testFileTypeAssociation() {
        // Create a temporary file with .typ extension
        val fileName = "test.typ"
        myFixture.configureByText(fileName, "")

        // Get the virtual file
        val virtualFile = myFixture.file.virtualFile

        // Verify that the file is recognized as a Typst file
        assertEquals(TypstFileType, virtualFile.fileType)
    }
}
