import type { PageInteractions } from "./interactions";

export type PreviewMode = "Doc" | "Slide";
export type InvertColorStrategy = "never" | "auto" | "always";
export type InvertColorStrategyMap = Partial<Record<"rest" | "image", InvertColorStrategy>>;

export interface PreviewArgs {
  ws: string;
  mode: string;
  state: string;
}

export interface PageSpec {
  index: number;
  width: number;
  height: number;
  pixelPerPt: number;
}

export interface PageRecord {
  index: number;
  key: string;
  container: HTMLDivElement;
  shell: HTMLDivElement;
  canvas: HTMLCanvasElement;
  linkLayer: HTMLDivElement;
  interactionLayer: HTMLDivElement;
  cursor: HTMLDivElement;
  jumpMarker: HTMLDivElement;
  interactions?: PageInteractions;
  transferred: boolean;
  width: number;
  height: number;
  fullWidthPx: number;
  fullHeightPx: number;
  cssWidth: number;
  cssHeight: number;
  pixelPerPt: number;
}

export interface PreviewPosition {
  page: number;
  x: number;
  y: number;
}

export interface VscodeApi {
  postMessage?: (message: unknown) => void;
  setState?: (state: unknown) => void;
  getState?: () => unknown;
}

export interface ViewportSnapshot {
  width: number;
  height: number;
  scrollLeft: number;
  scrollTop: number;
  devicePixelRatio: number;
  dragging?: boolean;
  scrolling?: boolean;
  renderDuringDrag?: boolean;
  window: {
    innerWidth: number;
    innerHeight: number;
  };
  boundingRect: DOMRectInit;
}

declare global {
  interface Window {
    __TYPST_PREVIEW_NG_ARGS__?: PreviewArgs;
  }

  const acquireVsCodeApi: undefined | (() => VscodeApi);
}
