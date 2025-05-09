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
import com.intellij.ui.jcef.JBCefBrowser
import org.cef.browser.CefBrowser
import org.cef.browser.CefFrame
import org.cef.handler.CefLoadHandler
import org.cef.network.CefRequest
import java.beans.PropertyChangeListener
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import javax.swing.JComponent

// This version ONLY loads the fixed URL for the background tinymist preview server.

class TypstPreviewFileEditor(private val project: Project, private val virtualFile: VirtualFile) : FileEditor {

    private val jbCefBrowser: JBCefBrowser
    private val component: JComponent

    // Define the Tinymist preview URL (default background port)
    private val previewHost = "127.0.0.1"
    private val previewPort = 23635
    private val tinymistPreviewUrl = "http://$previewHost:$previewPort"

    init {
        println("TypstPreviewFileEditor: Initializing JBCefBrowser...")
        jbCefBrowser = JBCefBrowser()
        component = jbCefBrowser.component

        jbCefBrowser.jbCefClient.addLoadHandler(object : CefLoadHandler {
            override fun onLoadStart(browser: CefBrowser?, frame: CefFrame?, transitionType: CefRequest.TransitionType?) {
                println("JCEF LoadHandler: onLoadStart - URL: ${frame?.url}, MainFrame: ${frame?.isMain}")
            }

            override fun onLoadingStateChange(browser: CefBrowser?, isLoading: Boolean, canGoBack: Boolean, canGoForward: Boolean) {
                println("JCEF LoadHandler: onLoadingStateChange - isLoading: $isLoading")
            }

            override fun onLoadEnd(browser: CefBrowser?, frame: CefFrame?, httpStatusCode: Int) {
                println("JCEF LoadHandler: onLoadEnd - URL: ${frame?.url}, Status: $httpStatusCode, MainFrame: ${frame?.isMain}")
            }

            override fun onLoadError(browser: CefBrowser?, frame: CefFrame?, errorCode: CefLoadHandler.ErrorCode?, errorText: String?, failedUrl: String?) {
                println("JCEF LoadHandler: onLoadError - URL: ${frame?.url}, ErrorCode: $errorCode, ErrorText: $errorText, FailedURL: $failedUrl, MainFrame: ${frame?.isMain}")
            }
        }, jbCefBrowser.cefBrowser)

        waitForServerAndLoadUrl()

        Disposer.register(this, jbCefBrowser)
        println("TypstPreviewFileEditor: Initialization setup complete.")
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

    private fun waitForServerAndLoadUrl() {
        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "Waiting for Tinymist Preview Server", false) {
            override fun run(indicator: ProgressIndicator) {
                indicator.isIndeterminate = true
                val startTime = System.currentTimeMillis()
                val timeoutMs = 30000

                while (System.currentTimeMillis() - startTime < timeoutMs) {
                    indicator.checkCanceled()
                    if (isServerReady()) {
                        println("TypstPreviewFileEditor: Preview server detected on port $previewPort.")
                        ApplicationManager.getApplication().invokeLater {
                            println("TypstPreviewFileEditor: Loading URL: $tinymistPreviewUrl")
                            if (!Disposer.isDisposed(this@TypstPreviewFileEditor)) {
                                jbCefBrowser.loadURL(tinymistPreviewUrl)
                            }
                        }
                        return
                    }
                    try {
                        Thread.sleep(500)
                    } catch (e: InterruptedException) {
                        Thread.currentThread().interrupt()
                        println("TypstPreviewFileEditor: Wait interrupted.")
                        break
                    }
                }
                println("TypstPreviewFileEditor: Timeout waiting for preview server on port $previewPort.")
            }
        })
    }

    override fun getComponent(): JComponent = component

    override fun getPreferredFocusedComponent(): JComponent = component

    override fun getName(): String = "Tinymist Preview"

    override fun setState(state: FileEditorState) {}

    override fun isModified(): Boolean = false

    override fun isValid(): Boolean = true

    override fun addPropertyChangeListener(listener: PropertyChangeListener) {}

    override fun removePropertyChangeListener(listener: PropertyChangeListener) {}

    override fun getFile(): VirtualFile = virtualFile

    override fun dispose() {
        // Disposer takes care of browser disposal
    }

    // UserDataHolder methods
    private val userData = mutableMapOf<Key<*>, Any?>()
    override fun <T : Any?> getUserData(key: Key<T>): T? {
        @Suppress("UNCHECKED_CAST")
        return userData[key] as T?
    }

    override fun <T : Any?> putUserData(key: Key<T>, value: T?) {
        userData[key] = value
    }
} 