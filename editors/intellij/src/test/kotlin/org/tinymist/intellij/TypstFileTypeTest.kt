package org.tinymist.intellij

import com.intellij.openapi.fileTypes.FileTypeManager
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
        val fileType = FileTypeManager.getInstance().getFileTypeByFileName("test.typ")
        assertEquals(TypstFileType, fileType)
    }
}
