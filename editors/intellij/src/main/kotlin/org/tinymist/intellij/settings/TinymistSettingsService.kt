package org.tinymist.intellij.settings

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.util.xmlb.XmlSerializerUtil
import java.io.IOException
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket

@State(
    name = "org.tinymist.intellij.settings.TinymistSettingsState",
    storages = [Storage("tinymistSettings.xml")]
)
class TinymistSettingsService : PersistentStateComponent<TinymistSettingsState> {

    private var internalState = TinymistSettingsState()
    
    // Session-only port storage (not persisted across IDE restarts)
    @Volatile
    private var sessionPreviewPort: Int = 0

    companion object {
        val instance: TinymistSettingsService
            get() = ApplicationManager.getApplication().getService(TinymistSettingsService::class.java)
    }

    override fun getState(): TinymistSettingsState {
        return internalState
    }

    override fun loadState(state: TinymistSettingsState) {
        XmlSerializerUtil.copyBean(state, internalState)
    }

    // Convenience accessors for settings
    var tinymistExecutablePath: String
        get() = internalState.tinymistExecutablePath
        set(value) {
            internalState.tinymistExecutablePath = value
        }

    var serverManagementMode: ServerManagementMode
        get() = internalState.serverManagementMode
        set(value) {
            internalState.serverManagementMode = value
        }
    
    // Convenience methods for checking management mode
    val isAutoManaged: Boolean
        get() = serverManagementMode == ServerManagementMode.AUTO_MANAGE
        
    val isCustomPath: Boolean
        get() = serverManagementMode == ServerManagementMode.CUSTOM_PATH
    
    // Preview port management (session-only, not persisted)
    var previewPort: Int
        get() = sessionPreviewPort
        set(value) {
            sessionPreviewPort = value
        }
    
    /**
     * Gets an available preview port, discovering one if not already set.
     * This method is thread-safe and will only discover a port once per application session.
     */
    @Synchronized
    fun getOrDiscoverPreviewPort(): Int {
        if (previewPort <= 0) {
            previewPort = findUnusedPort()
        }
        return previewPort
    }
    
    /**
     * Finds an unused port in the user port range (1024-65535).
     * Starts from the default Tinymist port 23635 and searches upward.
     */
    private fun findUnusedPort(): Int {
        val startPort = 23635 // Start from the default Tinymist preview port
        val endPort = 65535
        
        for (port in startPort..endPort) {
            if (isPortAvailable(port)) {
                return port
            }
        }
        
        // Fallback: search from 1024 upward if no ports found in preferred range
        for (port in 1024 until startPort) {
            if (isPortAvailable(port)) {
                return port
            }
        }
        
        // Ultimate fallback - this should rarely happen
        return startPort
    }
    
    /**
     * Checks if a port is available by first trying to connect to it (to detect existing services)
     * and then attempting to bind to it (to ensure we can actually use it).
     */
    private fun isPortAvailable(port: Int): Boolean {
        // First, check if there's already a service listening on this port
        try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress("127.0.0.1", port), 100)
                // If connection succeeds, the port is occupied by another service
                return false
            }
        } catch (_: IOException) {
            // Connection failed, which is good - no service is running on this port
            // Continue to check if we can bind to it
        }
        
        // Second, try to bind to the port to ensure we can actually use it
        return try {
            ServerSocket(port).use { true }
        } catch (_: IOException) {
            false
        }
    }
}