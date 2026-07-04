interface PreviewSocketOptions {
  post: (message: unknown) => void;
  onDocumentFrame: (kind: string, payload: Uint8Array, frameBytes: number) => Promise<void>;
  onPreviewMessage: (kind: string, text: string) => void;
  shouldIgnorePreviewMessage: (kind: string) => boolean;
}

const decoder = new TextDecoder();
const comma = ",".charCodeAt(0);
const notAvailable = "current not available";

/** Owns the preview WebSocket in the worker, handling server frames and outbound preview protocol messages. */
export class PreviewSocket {
  private socket: WebSocket | undefined;
  private reconnectTimer = 0;
  private disposed = false;
  private connectionSerial = 0;

  constructor(private readonly options: PreviewSocketOptions) {}

  setDisposed(disposed: boolean) {
    this.disposed = disposed;
  }

  async connect(wsUrl: string) {
    const connectionId = ++this.connectionSerial;
    this.clearReconnectTimer();
    this.closeSocket();

    if (!wsUrl) {
      this.options.post({ type: "status", state: "idle", message: "waiting" });
      return;
    }

    this.options.post({ type: "status", state: "connecting", message: wsUrl });

    const startedAt = performance.now();
    this.socket = new WebSocket(wsUrl);
    this.socket.binaryType = "arraybuffer";
    this.socket.addEventListener("open", () => {
      if (connectionId !== this.connectionSerial) {
        return;
      }
      this.options.post({ type: "socket-open", elapsedMs: performance.now() - startedAt });
      this.socket?.send("current");
    });
    this.socket.addEventListener("message", (event) => {
      if (connectionId !== this.connectionSerial) {
        return;
      }
      void this.handleSocketMessage(event.data);
    });
    this.socket.addEventListener("close", (event) => {
      if (connectionId !== this.connectionSerial) {
        return;
      }
      this.options.post({
        type: "socket-close",
        code: event.code,
        reason: event.reason,
        wasClean: event.wasClean,
      });
      if (!this.disposed) {
        this.reconnectTimer = setTimeout(() => void this.connect(wsUrl), 1000) as unknown as number;
      }
    });
    this.socket.addEventListener("error", () => {
      if (connectionId !== this.connectionSerial) {
        return;
      }
      this.options.post({ type: "error", message: "websocket error" });
    });
  }

  send(text: unknown) {
    if (typeof text === "string" && this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(text);
    }
  }

  dispose() {
    this.disposed = true;
    this.connectionSerial += 1;
    this.clearReconnectTimer();
    this.closeSocket();
  }

  private async handleSocketMessage(data: string | ArrayBuffer | Blob) {
    const startedAt = performance.now();
    if (!(data instanceof ArrayBuffer)) {
      if (data === notAvailable) {
        return;
      }

      this.options.post({
        type: "frame",
        kind: String(data),
        byteLength: typeof data === "string" ? data.length : 0,
        elapsedMs: performance.now() - startedAt,
      });
      return;
    }

    const bytes = new Uint8Array(data);
    const commaIndex = bytes.indexOf(comma);
    const kind =
      commaIndex >= 0 ? decoder.decode(bytes.slice(0, commaIndex)) : "unknown-binary-frame";
    const payload = commaIndex >= 0 ? bytes.slice(commaIndex + 1) : bytes;
    this.options.post({
      type: "frame",
      kind,
      byteLength: bytes.byteLength,
      payloadLength: payload.byteLength,
      elapsedMs: performance.now() - startedAt,
    });

    if (kind === "new" || kind === "diff-v1") {
      await this.options.onDocumentFrame(kind, payload, bytes.byteLength);
      return;
    }

    if (this.options.shouldIgnorePreviewMessage(kind)) {
      return;
    }

    this.options.onPreviewMessage(kind, decoder.decode(payload));
  }

  private closeSocket() {
    if (!this.socket) {
      return;
    }

    const closingSocket = this.socket;
    this.socket = undefined;
    closingSocket.close();
  }

  private clearReconnectTimer() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = 0;
    }
  }
}
