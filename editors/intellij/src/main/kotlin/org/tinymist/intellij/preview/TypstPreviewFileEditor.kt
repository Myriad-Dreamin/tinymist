package org.tinymist.intellij.preview

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileEditor.FileEditor
import com.intellij.openapi.fileEditor.FileEditorState
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.util.Key
import com.intellij.openapi.vfs.VirtualFile
import org.cef.browser.CefBrowser
import org.cef.browser.CefFrame
import org.cef.network.CefRequest
import java.beans.PropertyChangeListener
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import javax.swing.JComponent
import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JCEFHtmlPanel
import javax.swing.JLabel
import org.cef.handler.CefLoadHandlerAdapter
import org.cef.handler.CefLoadHandler
import com.intellij.openapi.util.registry.Registry

// This version ONLY loads the fixed URL for the background tinymist preview server.

class TypstPreviewFileEditor(
    private val project: Project,
    private val virtualFile: VirtualFile
) : JCEFHtmlPanel(isOsrEnabled(), null, null), FileEditor {

    // Define the Tinymist preview URL (default background port)
    private val previewHost = "127.0.0.1"
    private val previewPort = 23635
    private val tinymistPreviewUrl = "http://$previewHost:$previewPort"

    // Flag to track if the server check is complete and successful
    @Volatile
    private var isServerReady = false
    private var jcefUnsupportedLabel: JLabel? = null

    init {
        println("TypstPreviewFileEditor: Initializing (as JCEFHtmlPanel, non-OSR)...")

        if (!JBCefApp.isSupported()) {
            println("TypstPreviewFileEditor: JCEF is not supported! Preview will show an error message.")
            jcefUnsupportedLabel = JLabel("JCEF browser is not supported in this environment.")
        } else {
            println("TypstPreviewFileEditor: JCEF is supported. Setting up browser.")
            setupLoadHandler()
            waitForServerAndLoad()
        }
        println("TypstPreviewFileEditor: Initialization complete.")
    }

    private fun isServerReady(): Boolean {
        return try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress(previewHost, previewPort), 50)
                true
            }
        } catch (e: IOException) {
            false
        }
    }

    private fun waitForServerAndLoad() {
        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Waiting for Tinymist Server", false) {
            override fun run(indicator: ProgressIndicator) {
                var attempts = 0
                val maxAttempts = 60
                var serverFound = false
                while (attempts < maxAttempts && !serverFound && JBCefApp.isSupported()) {
                    indicator.checkCanceled()
                    try {
                        Socket().use { socket ->
                            socket.connect(InetSocketAddress(previewHost, previewPort), 500)
                            isServerReady = true
                            println("TypstPreviewFileEditor: Tinymist server is ready at $previewHost:$previewPort.")
                            serverFound = true
                        }
                    } catch (e: IOException) {
                        attempts++
                        indicator.text2 = "Attempt $attempts/$maxAttempts to connect to $previewHost:$previewPort"
                        Thread.sleep(500)
                    }
                }
            }

            override fun onSuccess() {
                if (!JBCefApp.isSupported()) return

                if (isServerReady) {
                    println("TypstPreviewFileEditor: Server ready, loading URL: $tinymistPreviewUrl")
                    this@TypstPreviewFileEditor.loadURL(tinymistPreviewUrl)
                } else {
                    println("TypstPreviewFileEditor: Server not ready. Displaying error.")
                    ApplicationManager.getApplication().invokeLater {
                        this@TypstPreviewFileEditor.loadHTML("<html><body>Error: Tinymist server not available at $previewHost:$previewPort. Please check if tinymist is running.</body></html>")
                    }
                }
            }

            override fun onThrowable(error: Throwable) {
                if (!JBCefApp.isSupported()) return

                println("TypstPreviewFileEditor: Error waiting for server: ${error.message}")
                ApplicationManager.getApplication().invokeLater {
                    this@TypstPreviewFileEditor.loadHTML("<html><body>Error connecting to Tinymist server: ${error.message}</body></html>")
                }
            }
        })
    }

    override fun getComponent(): JComponent {
        if (jcefUnsupportedLabel != null) {
            return jcefUnsupportedLabel!!
        }
        return super.getComponent()
    }

    override fun getPreferredFocusedComponent(): JComponent? {
        if (jcefUnsupportedLabel != null) {
            return jcefUnsupportedLabel!!
        }
        return super.getComponent()
    }

    override fun getName(): String = "Tinymist Preview"

    override fun setState(state: FileEditorState) {}

    override fun isModified(): Boolean = false

    override fun isValid(): Boolean = true

    override fun addPropertyChangeListener(listener: PropertyChangeListener) {}

    override fun removePropertyChangeListener(listener: PropertyChangeListener) {}

    override fun getFile(): VirtualFile = virtualFile

    private val userData = mutableMapOf<Key<*>, Any?>()
    override fun <T : Any?> getUserData(key: Key<T>): T? {
        @Suppress("UNCHECKED_CAST")
        return userData[key] as T?
    }

    override fun <T : Any?> putUserData(key: Key<T>, value: T?) {
        userData[key] = value
    }

    private fun setupLoadHandler() {
        this.jbCefClient.addLoadHandler(object : CefLoadHandlerAdapter() {
            override fun onLoadingStateChange(browser: CefBrowser?, isLoading: Boolean, canGoBack: Boolean, canGoForward: Boolean) {
                println("JCEF LoadHandler: onLoadingStateChange - isLoading: $isLoading")
            }

            override fun onLoadStart(browser: CefBrowser?, frame: CefFrame?, transitionType: CefRequest.TransitionType?) {
                println("JCEF LoadHandler: onLoadStart - URL: ${frame?.url ?: "N/A"}, MainFrame: ${frame?.isMain ?: "N/A"}")
            }

            override fun onLoadEnd(browser: CefBrowser?, frame: CefFrame?, httpStatusCode: Int) {
                println("JCEF LoadHandler: onLoadEnd - URL: ${frame?.url ?: "N/A"}, Status: $httpStatusCode, MainFrame: ${frame?.isMain ?: "N/A"}")
            }

            override fun onLoadError(browser: CefBrowser, frame: CefFrame, errorCode: CefLoadHandler.ErrorCode, errorText: String, failedUrl: String) {
                 println("JCEF LoadHandler: onLoadError - ErrorCode: $errorCode, Text: $errorText, URL: $failedUrl, MainFrame: ${frame.isMain}")
            }
        }, this.cefBrowser)
    }

    companion object {
        // Default to false (non-OSR) if the key isn't set.
        // Using the Markdown plugin's key for testing.
        private fun isOsrEnabled(): Boolean = Registry.`is`("ide.browser.jcef.markdownView.osr.enabled", false)
    }
} 