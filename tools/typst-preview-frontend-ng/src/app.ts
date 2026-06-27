import RendererWorker from "../renderer-worker?worker&inline";
import { collectPreviewElements, type PreviewElements } from "./dom";
import { PageHost } from "./page-host";
import type { PreviewArgs, PreviewMode, VscodeApi } from "./types";
import {
  acquireVscodeApi,
  parsePreviewMode,
  parsePreviewState,
  resolveWebSocketUrl,
} from "./utils";

interface StartPreviewAppOptions {
  rendererWasmUrl: string;
}

export function startPreviewApp({ rendererWasmUrl }: StartPreviewAppOptions) {
  const args = window.__TYPST_PREVIEW_NG_ARGS__ || {
    ws: "ws://127.0.0.1:23625",
    mode: "preview-arg:previewMode:Doc",
    state: "preview-arg:state:",
  };
  const app = new PreviewApp(args, rendererWasmUrl);
  void app.boot();
}

/** Coordinates the preview webview, handling worker messages, VS Code state, and page-host UI updates. */
class PreviewApp {
  private readonly elements: PreviewElements = collectPreviewElements();
  private readonly vscode: VscodeApi | undefined = acquireVscodeApi();
  private readonly initialPreviewState: unknown;
  private readonly pageHost: PageHost;
  private worker: Worker | undefined;
  private currentPreviewMode: PreviewMode;

  constructor(
    private readonly args: PreviewArgs,
    private readonly rendererWasmUrl: string,
  ) {
    this.initialPreviewState = parsePreviewState(args.state);
    this.currentPreviewMode = parsePreviewMode(args.mode);
    this.pageHost = new PageHost({
      elements: this.elements,
      postWorker: (message, transfer) => this.postWorker(message, transfer),
    });
  }

  async boot() {
    this.pageHost.installControls();
    this.pageHost.setPreviewMode(this.currentPreviewMode);
    this.pageHost.setContentPreview(false);

    if (this.initialPreviewState !== undefined) {
      this.vscode?.setState?.(this.initialPreviewState);
    }

    this.worker = new RendererWorker({ name: "typst-preview-ng-worker" });
    this.worker.addEventListener("message", (event) => this.handleWorkerMessage(event));
    this.worker.addEventListener("error", (event) => {
      console.error("[typst-preview-ng]", event.message || "worker error", event);
    });

    this.postWorker({
      type: "init",
      wsUrl: resolveWebSocketUrl(this.args.ws),
      rendererWasmUrl: new URL(this.rendererWasmUrl, window.location.href).href,
      previewMode: this.currentPreviewMode,
      previewState: this.initialPreviewState,
      isContentPreview: false,
      viewport: this.pageHost.readViewportSnapshot(),
    });

    this.vscode?.postMessage?.({ type: "started" });
    window.addEventListener("resize", () =>
      this.pageHost.scheduleViewportSnapshot({ applyLayouts: true }),
    );
    this.elements.viewport.addEventListener(
      "scroll",
      () => this.pageHost.scheduleViewportSnapshot({ scrolling: true }),
      { passive: true },
    );
    window.addEventListener("beforeunload", () => this.disposeWorker());
    window.addEventListener("message", (event) => this.handleExtensionMessage(event));
  }

  private handleWorkerMessage(event: MessageEvent) {
    const message = event.data || {};
    switch (message.type) {
      case "status":
      case "renderer-ready":
      case "socket-open":
      case "socket-close":
      case "frame":
        break;
      case "preview-message":
        this.pageHost.handlePreviewProtocolMessage(message.kind, message.text || "");
        break;
      case "ensure-pages":
        this.handleEnsurePages(message);
        break;
      case "render-complete":
        this.pageHost.markRendered(
          message.generation,
          message.layer,
          message.quality,
          message.pageIndices || [],
          message.fullPageIndices || [],
        );
        this.pageHost.updateInteractions(
          message.generation,
          message.interactions || [],
          message.invalidatedInteractions || [],
        );
        break;
      case "render-evicted":
        this.pageHost.markEvicted(message.generation, message.pageIndices || []);
        break;
      case "interactions":
        this.pageHost.updateInteractions(message.generation, message.interactions || []);
        break;
      case "text-hit":
        this.pageHost.handleTextHit(message);
        break;
      case "text-rect":
        this.pageHost.handleTextRect(message);
        break;
      case "bound-hit":
        this.pageHost.handleBoundHit(message);
        break;
      case "error":
        console.error("[typst-preview-ng]", message.message, message);
        break;
    }
  }

  private handleEnsurePages(message: any) {
    try {
      this.pageHost.ensurePages(message.generation, message.pages || []);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      this.postWorker({
        type: "canvases-error",
        generation: message.generation,
        message: detail,
      });
      console.error("[typst-preview-ng] failed to prepare pages", error);
    }
  }

  private handleExtensionMessage(event: MessageEvent) {
    const message = event.data || {};
    switch (message.type) {
      case "reconnect": {
        const nextMode = parsePreviewMode(`preview-arg:previewMode:${message.mode || "Doc"}`);
        this.currentPreviewMode = nextMode;
        this.pageHost.setPreviewMode(nextMode);
        this.pageHost.setContentPreview(!!message.isContentPreview);
        this.postWorker({
          type: "reconnect",
          wsUrl: resolveWebSocketUrl(message.url || ""),
          previewMode: nextMode,
          isContentPreview: !!message.isContentPreview,
          viewport: this.pageHost.readViewportSnapshot(),
        });
        if (!message.url) {
          this.pageHost.clearPages();
        }
        break;
      }
      case "outline":
        if (message.isContentPreview) {
          this.pageHost.setContentPreview(true);
        }
        this.pageHost.setOutlineData(message.outline);
        break;
    }
  }

  private postWorker(message: unknown, transfer?: Transferable[]) {
    if (transfer) {
      this.worker?.postMessage(message, transfer);
      return;
    }
    this.worker?.postMessage(message);
  }

  private disposeWorker() {
    this.postWorker({ type: "dispose" });
    this.worker?.terminate();
    this.worker = undefined;
  }
}
