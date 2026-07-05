/// <reference lib="webworker" />

import { WorkerRenderer } from "./src/worker/rendering";
import { PreviewSocket } from "./src/worker/socket";

declare const self: DedicatedWorkerGlobalScope & {
  loadSvg?: (
    data: BufferSource,
    format: string,
    width?: number,
    height?: number,
  ) => Promise<ImageBitmap>;
};

interface PendingSvgLoad {
  resolve: (bitmap: ImageBitmap) => void;
  reject: (error: Error) => void;
  timeout: number;
}

let nextSvgLoadRequestId = 0;
const pendingSvgLoads = new Map<number, PendingSvgLoad>();

function copyBufferSource(data: BufferSource): ArrayBuffer {
  const source =
    data instanceof ArrayBuffer
      ? new Uint8Array(data)
      : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  const copy = new Uint8Array(source.byteLength);
  copy.set(source);
  return copy.buffer;
}

self.loadSvg = (data, format, width, height) => {
  const requestId = ++nextSvgLoadRequestId;
  const bytes = copyBufferSource(data);

  return new Promise<ImageBitmap>((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingSvgLoads.delete(requestId);
      reject(new Error(`timed out decoding SVG image ${requestId}`));
    }, 30_000) as unknown as number;

    pendingSvgLoads.set(requestId, { resolve, reject, timeout });
    try {
      postMessage(
        {
          type: "load-svg",
          requestId,
          data: bytes,
          format,
          width,
          height,
        },
        [bytes],
      );
    } catch (error) {
      clearTimeout(timeout);
      pendingSvgLoads.delete(requestId);
      reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
};

let currentConfig: any;

const renderer = new WorkerRenderer((message) => postMessage(message));
const socket = new PreviewSocket({
  post: (message) => postMessage(message),
  onDocumentFrame: (kind, payload, frameBytes) =>
    renderer.processDocumentFrame(kind, payload, frameBytes),
  onPreviewMessage: (kind, text) => postMessage({ type: "preview-message", kind, text }),
  shouldIgnorePreviewMessage(kind) {
    return (
      !!currentConfig?.isContentPreview &&
      (kind === "viewport" || kind === "partial-rendering" || kind === "cursor")
    );
  },
});

self.addEventListener("message", (event) => {
  const message = event.data || {};
  switch (message.type) {
    case "init":
      currentConfig = message;
      renderer.configure(message);
      socket.setDisposed(false);
      if (message.wsUrl) {
        void renderer.ensureSessionReady();
      }
      void socket.connect(message.wsUrl);
      break;
    case "reconnect":
      currentConfig = { ...currentConfig, ...message };
      renderer.configure(currentConfig);
      renderer.resetDocumentState();
      socket.setDisposed(false);
      if (message.wsUrl) {
        void renderer.ensureSessionReady();
      }
      void socket.connect(message.wsUrl);
      break;
    case "viewport":
      currentConfig = { ...currentConfig, viewport: message.viewport };
      renderer.setViewport(message.viewport, message.layouts);
      break;
    case "send":
      socket.send(message.text);
      break;
    case "canvases":
      renderer.acceptCanvases(message.generation, message.canvases || [], message.ack);
      break;
    case "canvases-error":
      renderer.rejectCanvases(
        message.generation,
        new Error(
          message.message || `failed to prepare canvases for generation ${message.generation}`,
        ),
      );
      break;
    case "load-svg-result": {
      const pending = pendingSvgLoads.get(message.requestId);
      if (pending) {
        clearTimeout(pending.timeout);
        pendingSvgLoads.delete(message.requestId);
        pending.resolve(message.bitmap);
      }
      break;
    }
    case "load-svg-error": {
      const pending = pendingSvgLoads.get(message.requestId);
      if (pending) {
        clearTimeout(pending.timeout);
        pendingSvgLoads.delete(message.requestId);
        pending.reject(new Error(message.message || "failed to decode SVG image"));
      }
      break;
    }
    case "request-interactions":
      void renderer.requestInteractions({
        generation: message.generation,
        pageIndices: message.pageIndices || [],
      });
      break;
    case "hit-bound":
      void renderer.hitBound({
        requestId: message.requestId,
        generation: message.generation,
        pageIndex: message.pageIndex,
        x: message.x,
        y: message.y,
      });
      break;
    case "hit-text":
      void renderer.hitText({
        requestId: message.requestId,
        generation: message.generation,
        pageIndex: message.pageIndex,
        x: message.x,
        y: message.y,
        rect: message.rect,
      });
      break;
    case "resolve-text-rect":
      void renderer.resolveTextRect({
        requestId: message.requestId,
        generation: message.generation,
        pageIndex: message.pageIndex,
        textId: message.textId,
        rect: message.rect,
      });
      break;
    case "dispose":
      socket.dispose();
      renderer.dispose();
      break;
  }
});
