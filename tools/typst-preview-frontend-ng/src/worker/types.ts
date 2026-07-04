import type { PageInteractions } from "../interactions";
import type { PageLayout, PageSpec } from "../types";

export type { PageLayout, PageSpec } from "../types";

export interface RenderRect {
  lo: { x: number; y: number };
  hi: { x: number; y: number };
}

export interface PageRenderSpec extends PageSpec {
  window?: RenderRect;
}

export type CanvasLayer = "full";
export type RenderQuality = "preview" | "full";

export interface CanvasEntry {
  canvas: OffscreenCanvas;
  context?: OffscreenCanvasRenderingContext2D;
  scratch?: OffscreenCanvas;
  scratchContext?: OffscreenCanvasRenderingContext2D;
  widthPx: number;
  heightPx: number;
  hasContent?: boolean;
  quality?: RenderQuality;
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
  quality?: RenderQuality;
  fullPage?: boolean;
  interactions?: PageInteractions;
  invalidatedInteractions?: number[];
}
