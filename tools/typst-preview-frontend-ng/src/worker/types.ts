export interface PageSpec {
  index: number;
  width: number;
  height: number;
  pixelPerPt: number;
}

export interface RenderRect {
  lo: { x: number; y: number };
  hi: { x: number; y: number };
}

export interface PageRenderSpec extends PageSpec {
  window?: RenderRect;
}

export interface PageLayout {
  index: number;
  top: number;
  bottom: number;
  height: number;
  scale: number;
}

export interface CanvasEntry {
  canvas: OffscreenCanvas;
  context?: OffscreenCanvasRenderingContext2D;
  widthPx: number;
  heightPx: number;
}

export interface CanvasAck {
  layouts?: PageLayout[];
}

export interface PendingCanvasAck {
  resolve: (ack: CanvasAck | null) => void;
  reject: (error: Error) => void;
  timeout: number;
}

export interface WorkerConfig {
  rendererWasmUrl?: string;
  viewport?: any;
  isContentPreview?: boolean;
}

export type WorkerPost = (message: unknown) => void;
