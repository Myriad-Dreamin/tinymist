package org.tinymist.intellij.preview

import com.intellij.openapi.diagnostic.Logger
import io.netty.channel.ChannelHandlerContext
import io.netty.handler.codec.http.FullHttpRequest
import io.netty.handler.codec.http.QueryStringDecoder
import org.jetbrains.ide.HttpRequestHandler
import java.io.IOException
import com.intellij.util.PathUtilRt
import io.netty.handler.codec.http.HttpHeaderNames
import io.netty.handler.codec.http.HttpResponseStatus
import io.netty.handler.codec.http.HttpUtil
import org.jetbrains.io.FileResponses
import org.jetbrains.io.Responses

private val LOG_HANDLER = Logger.getInstance(TypstPreviewResourceHandler::class.java)

class TypstPreviewResourceHandler : HttpRequestHandler() {

    companion object {
        // This prefix should match the one used in TypstPreviewFileEditor and plugin.xml for the EP
        private const val HANDLER_PREFIX = PREVIEW_RESOURCE_PREFIX // From TypstPreviewFileEditor
    }

    override fun isSupported(request: FullHttpRequest): Boolean {
        return request.uri().startsWith(HANDLER_PREFIX)
    }

    override fun process(
        urlDecoder: QueryStringDecoder,
        request: FullHttpRequest,
        context: ChannelHandlerContext
    ): Boolean {
        val requestPath = urlDecoder.path()
        if (!requestPath.startsWith(HANDLER_PREFIX)) {
            LOG_HANDLER.warn("Request path $requestPath does not start with prefix $HANDLER_PREFIX")
            return false
        }

        // Map the request path to a resource path within our plugin's bundled assets
        // e.g., /typst-intellij-plugin-assets/typst-webview-assets/main.js -> /typst_preview_frontend/typst-webview-assets/main.js
        val resourceRelativePath = requestPath.substring(HANDLER_PREFIX.length)
        // Ensure leading slash for resource path
        val safeRelativePath = if (resourceRelativePath.startsWith("/")) resourceRelativePath else "/$resourceRelativePath"
        val fullResourcePath = "/typst_preview_frontend$safeRelativePath"

        LOG_HANDLER.info("Attempting to serve resource: $fullResourcePath (from request: $requestPath)")

        try {
            val resourceUrl = TypstPreviewResourceHandler::class.java.getResource(fullResourcePath)
            if (resourceUrl == null) {
                LOG_HANDLER.warn("Resource not found: $fullResourcePath")
                // Added request parameter
                Responses.sendNotFoundError(context.channel(), request)
                return true
            }

            val stream = resourceUrl.openStream() ?: run {
                LOG_HANDLER.warn("Cannot open stream for resource: $fullResourcePath")
                // Added request parameter
                Responses.sendNotFoundError(context.channel(), request)
                return true
            }

            val bytes = stream.readAllBytes()
            stream.close()

            // Determine content type (important for WASM, JS, CSS)
            val contentType = FileResponses.getContentType(PathUtilRt.getFileName(fullResourcePath))
            LOG_HANDLER.debug("Serving $fullResourcePath with Content-Type: $contentType")

            val response = Responses.response(contentType, io.netty.buffer.Unpooled.wrappedBuffer(bytes))
            // Disable caching for preview assets to ensure updates are reflected
            response.headers().set(HttpHeaderNames.CACHE_CONTROL, "no-cache, no-store, must-revalidate")
            response.headers().set(HttpHeaderNames.PRAGMA, "no-cache")
            response.headers().set(HttpHeaderNames.EXPIRES, "0")
            HttpUtil.setContentLength(response, bytes.size.toLong())

            // Added request parameter
            Responses.send(response, context.channel(), request)
            LOG_HANDLER.info("Successfully served resource: $fullResourcePath")
            return true

        } catch (e: IOException) {
            LOG_HANDLER.error("Error serving resource $fullResourcePath", e)
            // Added request parameter
            Responses.sendStatus(HttpResponseStatus.INTERNAL_SERVER_ERROR, context.channel(), request)
            return true
        }
    }
}