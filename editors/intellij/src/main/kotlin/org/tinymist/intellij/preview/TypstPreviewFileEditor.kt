package org.tinymist.intellij.preview

import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.fileEditor.FileEditor
import com.intellij.openapi.fileEditor.FileEditorLocation
import com.intellij.openapi.fileEditor.FileEditorState
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.util.UserDataHolderBase
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JBCefBrowser
import com.intellij.ui.jcef.JBCefJSQuery
import com.intellij.util.ui.UIUtil
import org.jetbrains.ide.BuiltInServerManager
import org.tinymist.intellij.TypstFileType
import java.beans.PropertyChangeListener
import java.io.InputStreamReader
import javax.swing.JComponent
import javax.swing.JLabel
import javax.swing.JPanel
import com.intellij.openapi.fileEditor.FileEditorPolicy
import com.intellij.openapi.fileEditor.FileEditorProvider
import com.intellij.openapi.project.DumbAware

private val LOG = Logger.getInstance(TypstPreviewFileEditor::class.java)

// Define a unique prefix for our plugin's resource server
const val PREVIEW_RESOURCE_PREFIX = "/typst-intellij-plugin-assets"

class TypstPreviewFileEditor(
    private val project: Project,
    private val virtualFile: VirtualFile
) : UserDataHolderBase(), FileEditor, Disposable {

    private var jbCefBrowser: JBCefBrowser? = null
    private val panel: JComponent
    private var jsQuery: JBCefJSQuery? = null

    private val htmlTemplatePath = "/typst_preview_frontend/index.html" // Path within plugin resources

    init {
        if (JBCefApp.isSupported()) {
            jbCefBrowser = JBCefBrowser.createBuilder()
                // .setOffScreenRendering(false) // Useful for devtools; consider if needed
                .build()
            panel = jbCefBrowser!!.component

            jsQuery = JBCefJSQuery.create(jbCefBrowser!!)
            jsQuery!!.addHandler { message: String ->
                LOG.info("Message from JCEF: $message")
                // TODO: Parse message (e.g., JSON) from actual typst-preview JS
                // and handle scroll events, clicks, custom events, etc.
                null
            }

            loadPageWithLocalAssets()
            applyTheme(UIUtil.isUnderDarcula())

        } else {
            panel = JPanel().apply {
                add(JLabel("JCEF browser is not supported/enabled. Typst preview unavailable."))
            }
        }
    }

    private fun loadPageWithLocalAssets() {
        try {
            val resourceHandlerPort = BuiltInServerManager.getInstance().port
            val pluginAssetBaseUrl = "http://localhost:$resourceHandlerPort$PREVIEW_RESOURCE_PREFIX"

            // 1. Load the index.html template from plugin resources
            var htmlTemplateStream = TypstPreviewFileEditor::class.java.getResourceAsStream(htmlTemplatePath)
            if (htmlTemplateStream == null) {
                LOG.error("Cannot find preview template: $htmlTemplatePath")
                jbCefBrowser?.loadHTML("<html><body>Error: Preview HTML template not found.</body></html>")
                return
            }
            var htmlContent = InputStreamReader(htmlTemplateStream).readText()

            // 2. Replace asset placeholders (e.g., /typst-webview-assets/) with our local server URLs
            //    The actual placeholder will depend on the index.html from typst-preview
            htmlContent = htmlContent.replace("/typst-webview-assets/", "$pluginAssetBaseUrl/typst-webview-assets/")
            // Add more replacements if other asset base paths are used in the original index.html

            // 3. Inject our JBCefJSQuery bridge and any other IntelliJ-specific JS helpers
            //    The actual typst-preview JS should handle its own WebSocket connection to tinymist server.
            //    Our bridge is for secondary control (themes, editor-driven scroll, etc.)
            val jsIntellijBridge = """
                window.typstIntellij = {
                    sendMessageToKotlin: function(message) {
                        ${jsQuery!!.inject("message")}
                    },
                    // Example functions callable from Kotlin:
                    scrollToPercent: function(percent) {
                        // This function might already exist in typst-preview's JS, or we adapt.
                        // For now, a simple placeholder:
                        const h = document.documentElement, b = document.body, st = 'scrollTop', sh = 'scrollHeight';
                        const targetScroll = (h[sh]||b[sh]) * Math.max(0, Math.min(1, percent)) ;
                        h[st] = b[st] = targetScroll;
                        console.log('JS: Intellij scrolled to ' + (percent * 100) + '%');
                    },
                    applyTheme: function(themeName) { // e.g., 'theme-light' or 'theme-dark'
                        document.body.classList.remove('theme-light', 'theme-dark');
                        document.body.classList.add(themeName);
                        console.log('JS: Intellij applied theme ' + themeName);
                    }
                };
                // Example: Listen to scroll events from the preview's content and report back
                // This might need to be adapted based on how typst-preview structures its scrollable areas.
                // Assuming a main scrollable element or window scroll:
                (document.getElementById('main-preview-area') || window).addEventListener('scroll', () => {
                    const h = document.documentElement, b = document.body, st = 'scrollTop', sh = 'scrollHeight';
                    const el = document.getElementById('main-preview-area') || h;
                    const elementHeight = el.clientHeight;
                    const scrollableHeight = (el.scrollHeight || b.scrollHeight);
                    if (scrollableHeight - elementHeight === 0) { // Avoid division by zero if not scrollable
                         window.typstIntellij.sendMessageToKotlin(JSON.stringify({ type: 'scroll', data: { percent: 0 } }));
                         return;
                    }
                    const percent = Math.min(1, Math.max(0, (el.scrollTop||b.scrollTop) / (scrollableHeight - elementHeight)));
                    window.typstIntellij.sendMessageToKotlin(JSON.stringify({ type: 'scroll', data: { percent: percent } }));
                }, { passive: true });

                console.log('Typst IntelliJ JS bridge injected.');
            """
            // Inject this script. A common way is to add a <script> tag.
            // Ensuring it runs after the main typst-preview JS might require careful placement or events.
            htmlContent = htmlContent.replace("</body>", "<script>\n$jsIntellijBridge\n</script>\n</body>")

            // 4. Load the modified HTML. The pluginAssetBaseUrl serves as the base for resolving further relative paths.
            jbCefBrowser?.loadHTML(htmlContent, pluginAssetBaseUrl)
            LOG.info("Preview page loaded with processed template. Base URL: $pluginAssetBaseUrl")

        } catch (e: Exception) {
            LOG.error("Error loading initial preview page with local assets", e)
            jbCefBrowser?.loadHTML("<html><body>Error loading preview: ${e.message}</body></html>")
        }
    }

    // This method might not be needed if the core typst-preview client handles all rendering via WebSockets.
    // It could be used for overlays or if tinymist sends self-contained HTML snippets for some reason.
    fun updateRenderedContent(typstHtmlSnippet: String) {
        LOG.warn("updateRenderedContent is likely not the primary way to update preview with typst-preview architecture.")
        // If typst-preview JS exposes a function to directly inject a full HTML snippet:
        // val escapedHtml = typstHtmlSnippet.replace("'", "''").replace("\n", "\n")
        // val script = "window.typstPreviewClient.updateFullContent('\$escapedHtml');" // Assuming such a function
        // jbCefBrowser?.cefBrowser?.executeJavaScript(script, jbCefBrowser?.devToolsURL, 0)
    }

    fun postScrollTo(editorScrollPercent: Double) {
        val percent = editorScrollPercent.coerceIn(0.0, 1.0) // Assuming percent is 0.0 to 1.0
        val script = "window.typstIntellij && window.typstIntellij.scrollToPercent(\$percent);"
        jbCefBrowser?.cefBrowser?.executeJavaScript(script, null, 0)
        LOG.info("Posted scroll to JCEF: $percent")
    }

    fun applyTheme(isDark: Boolean) {
        val themeName = if (isDark) "theme-dark" else "theme-light"
        val script = "window.typstIntellij && window.typstIntellij.applyTheme(\'$themeName\');"
        jbCefBrowser?.cefBrowser?.executeJavaScript(script, null, 0)
        LOG.info("Applied theme to JCEF: $themeName")
    }

    override fun getComponent(): JComponent = panel
    override fun getPreferredFocusedComponent(): JComponent? = panel
    override fun getName(): String = "Typst Preview"
    override fun setState(state: FileEditorState) { /* TODO: Handle state persistence if needed */ }
    override fun isModified(): Boolean = false
    override fun isValid(): Boolean = virtualFile.isValid
    override fun addPropertyChangeListener(listener: PropertyChangeListener) {}
    override fun removePropertyChangeListener(listener: PropertyChangeListener) {}
    override fun getCurrentLocation(): FileEditorLocation? = null
    override fun getFile(): VirtualFile = virtualFile

    override fun dispose() {
        LOG.info("Disposing TypstPreviewFileEditor for ${virtualFile.name}")
        jsQuery?.dispose()
        jsQuery = null
        jbCefBrowser?.let { Disposer.dispose(it) }
        jbCefBrowser = null
    }

    // Companion object to act as the FileEditorProvider for this preview editor
    // This remains largely the same as it's standard IntelliJ plumbing.
    class Provider : FileEditorProvider, DumbAware {
        override fun accept(project: Project, file: VirtualFile): Boolean {
            return file.fileType is TypstFileType || file.extension?.equals(TypstFileType.defaultExtension, ignoreCase = true) == true
        }

        override fun createEditor(project: Project, file: VirtualFile): FileEditor {
            return TypstPreviewFileEditor(project, file)
        }

        override fun getEditorTypeId(): String {
            return "typst.preview.file.editor"
        }

        override fun getPolicy(): FileEditorPolicy {
            return FileEditorPolicy.PLACE_AFTER_DEFAULT_EDITOR
        }
    }
} 