import type { PageInteractions } from "../interactions";

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
  width: number;
  height: number;
  scale: number;
}

export interface CanvasEntry {
  canvas: OffscreenCanvas;
  context?: OffscreenCanvasRenderingContext2D;
  scratch?: OffscreenCanvas;
  scratchContext?: OffscreenCanvasRenderingContext2D;
  widthPx: number;
  heightPx: number;
  hasContent?: boolean;
  quality?: "preview" | "full";
  renderedGeneration?: number;
  renderedPixelPerPt?: number;
  renderedWindowKey?: string;
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
  previewMode?: string;
  isContentPreview?: boolean;
}

export type WorkerPost = (message: unknown) => void;

export interface PageRenderResult {
  pageIndex: number;
  quality?: "preview" | "full";
  fullPage?: boolean;
  interactions?: PageInteractions;
  invalidatedInteractions?: number[];
}
