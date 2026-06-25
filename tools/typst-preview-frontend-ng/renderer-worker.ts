/// <reference lib="webworker" />

import { WorkerRenderer } from "./src/worker/rendering";
import { PreviewSocket } from "./src/worker/socket";

declare const self: DedicatedWorkerGlobalScope & {
  loadSvg?: (
    data: BufferSource,
    format: string,
    width: number,
    height: number,
  ) => Promise<ImageBitmap>;
};

self.loadSvg = async (data, format) => {
  const type = format.includes("/") ? format : "image/svg+xml";
  const blob = new Blob([data], { type });
  return createImageBitmap(blob);
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
    case "dispose":
      socket.dispose();
      renderer.dispose();
      break;
  }
});
