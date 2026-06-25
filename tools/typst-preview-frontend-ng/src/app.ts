import RendererWorker from "../renderer-worker?worker&inline";
import { collectPreviewElements, type PreviewElements } from "./dom";
import { PageHost } from "./page-host";
import type { PreviewArgs, PreviewMode, VscodeApi } from "./types";
import {
  acquireVscodeApi,
  formatBytes,
  formatMs,
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
  private frameCount = 0;
  private byteCount = 0;
  private renderCount = 0;
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
      setLastMessage: (message) => this.setLastMessage(message),
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
      this.setSocketState("error");
      this.setLastMessage(event.message || "worker error");
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
    window.addEventListener("resize", () => this.pageHost.scheduleViewportSnapshot());
    this.elements.viewport.addEventListener(
      "scroll",
      () => this.pageHost.scheduleViewportSnapshot(),
      { passive: true },
    );
    window.addEventListener("beforeunload", () => this.disposeWorker());
    window.addEventListener("message", (event) => this.handleExtensionMessage(event));
  }

  private handleWorkerMessage(event: MessageEvent) {
    const message = event.data || {};
    switch (message.type) {
      case "status":
        this.setSocketState(message.state || "idle");
        this.setLastMessage(message.message || message.state || "");
        break;
      case "renderer-ready":
        this.setLastMessage("renderer ready");
        break;
      case "socket-open":
        this.setSocketState("open");
        this.setLastMessage("connected");
        break;
      case "socket-close":
        this.setSocketState("closed");
        this.setLastMessage(`closed ${message.code ?? ""}`.trim());
        break;
      case "frame":
        this.frameCount += 1;
        this.byteCount += message.byteLength || 0;
        this.elements.frameKind.textContent = message.kind || "frame";
        this.elements.frameSize.textContent = formatBytes(message.byteLength || 0);
        this.elements.frameCount.textContent = `frames: ${this.frameCount}`;
        this.elements.byteCount.textContent = `bytes: ${formatBytes(this.byteCount)}`;
        this.setLastMessage(`${message.kind || "frame"} @ ${formatMs(message.elapsedMs)}`);
        break;
      case "preview-message":
        this.pageHost.handlePreviewProtocolMessage(message.kind, message.text || "");
        break;
      case "ensure-pages":
        this.handleEnsurePages(message);
        break;
      case "render-complete":
        this.renderCount += 1;
        this.elements.renderCount.textContent = `renders: ${this.renderCount}`;
        this.setLastMessage(`rendered ${message.pageCount || 0} pages (${message.phase || "all"})`);
        break;
      case "error":
        this.setSocketState("error");
        this.setLastMessage(message.message || "worker error");
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
      this.setSocketState("error");
      this.setLastMessage(detail);
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
        this.resetCounters();
        this.postWorker({
          type: "reconnect",
          wsUrl: resolveWebSocketUrl(message.url || ""),
          previewMode: nextMode,
          isContentPreview: !!message.isContentPreview,
          viewport: this.pageHost.readViewportSnapshot(),
        });
        if (!message.url) {
          this.pageHost.clearPages();
          this.setSocketState("idle");
          this.setLastMessage("waiting");
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

  private resetCounters() {
    this.frameCount = 0;
    this.byteCount = 0;
    this.renderCount = 0;
    this.pageHost.resetCounters();
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

  private setSocketState(state: string) {
    this.elements.socketState.textContent = state;
    this.elements.socketState.dataset.state = state;
  }

  private setLastMessage(message: string) {
    this.elements.lastMessage.textContent = message || "";
  }
}
