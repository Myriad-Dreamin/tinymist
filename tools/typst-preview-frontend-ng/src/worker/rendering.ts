import {
  createTypstRenderer,
  type RenderSession,
  type TypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import * as rendererWrapper from "@myriaddreamin/typst-ts-renderer";
import {
  parsePageInteractions,
  type BoundInteraction,
  type PageInteractions,
  type PageRect,
} from "../interactions";
import type {
  CanvasAck,
  CanvasEntry,
  PageLayout,
  PageRenderSpec,
  PageRenderResult,
  PageSpec,
  PendingCanvasAck,
  WorkerConfig,
  WorkerPost,
} from "./types";

const minPixelPerPt = 0.5;
const maxPixelPerPt = 4;
const visibleRenderScreens = 1;
const scrollRenderScreens = 1;
const prefetchRenderScreens = 5;
const idlePrefetchDelayMs = 120;
const maxCanvasBufferPages = 18;
const interactivePixelPerPtScale = 0.65;
const minInteractivePixelPerPt = 0.7;
const maxInteractivePixelPerPt = 1.25;
type CanvasLayer = "full";
type RenderQuality = "preview" | "full";

interface RenderAndPostOptions {
  session: RenderSession;
  pages: PageRenderSpec[];
  generation: number;
  kind: string;
  frameBytes: number;
  phase: string;
  layer: CanvasLayer;
  quality: RenderQuality;
  useCacheKey: boolean;
  updateCacheKey: boolean;
  collectInteractions: boolean;
  prioritizeViewport: boolean;
  cancelOnViewportChange: boolean;
  pauseWhenDragging: boolean;
  viewportVersion: number;
  frameVersion: number;
  flushResults?: boolean;
}

interface RenderPagesOptions {
  useCacheKey: boolean;
  updateCacheKey: boolean;
  collectInteractions: boolean;
  prioritizeViewport: boolean;
  cancelOnViewportChange: boolean;
  pauseWhenDragging: boolean;
  viewportVersion: number;
  frameVersion: number;
  layer: CanvasLayer;
  quality: RenderQuality;
  generation: number;
  onResult?: (result: PageRenderResult) => void;
}

/** Owns Typst rendering in the worker, handling document frames, canvas acknowledgements, and viewport updates. */
export class WorkerRenderer {
  private disposed = false;
  private config: WorkerConfig = {};
  private generation = 0;
  private documentVersion = 0;
  private initializedDocument = false;
  private renderer: TypstRenderer | undefined;
  private releaseSession: (() => void) | undefined;
  private rendererWasmUrl = "";
  private sessionReady: Promise<RenderSession> | undefined;
  private renderQueue = Promise.resolve();
  private viewportVersion = 0;
  private viewportRenderQueued = false;
  private idlePrefetchTimer = 0;
  private latestPages: PageSpec[] = [];
  private readonly pageCanvases = new Map<number, CanvasEntry>();
  private readonly pageCanvasLru = new Map<number, true>();
  private readonly pageCacheKeys = new Map<number, string>();
  private readonly pageLatestCacheKeys = new Map<number, string>();
  private readonly pagePixelPerPt = new Map<number, number>();
  private readonly pageInteractionCacheKeys = new Map<number, string>();
  private readonly pageInteractions = new Map<number, PageInteractions>();
  private readonly pendingCanvasAcks = new Map<number, PendingCanvasAck>();
  private latestPageLayouts: PageLayout[] = [];

  constructor(private readonly post: WorkerPost) {}

  configure(config: WorkerConfig) {
    this.disposed = false;
    this.config = { ...this.config, ...config };
    this.rendererWasmUrl = config.rendererWasmUrl || this.rendererWasmUrl;
  }

  setViewport(viewport: any, layouts?: PageLayout[]) {
    this.clearIdlePrefetchTimer();
    this.config = { ...this.config, viewport };
    if (layouts) {
      this.latestPageLayouts = layouts;
    }
    this.viewportVersion += 1;
    if (!this.isViewportDragging() || this.isViewportScrolling()) {
      this.scheduleViewportRender();
    }
  }

  ensureSessionReady(): Promise<RenderSession> {
    if (!this.sessionReady) {
      this.sessionReady = (
        this.rendererWasmUrl
          ? this.initializeRenderer(this.rendererWasmUrl)
          : Promise.reject(new Error("typst.ts renderer wasm URL is not configured"))
      ).catch((error) => {
        this.postError(error);
        throw error;
      });
    }
    return this.sessionReady;
  }

  async processDocumentFrame(kind: string, payload: Uint8Array, frameBytes: number) {
    const frameVersion = ++this.documentVersion;
    this.renderQueue = this.renderQueue
      .then(() => this.renderDocumentFrame(kind, payload, frameBytes, frameVersion))
      .catch((error) => {
        this.postError(error);
      });
    await this.renderQueue;
  }

  private scheduleViewportRender() {
    if (
      this.disposed ||
      this.viewportRenderQueued ||
      this.generation <= 0 ||
      this.latestPages.length === 0
    ) {
      return;
    }

    this.viewportRenderQueued = true;
    const frameVersion = this.documentVersion;
    this.renderQueue = this.renderQueue
      .then(() => this.renderViewportUpdate(frameVersion))
      .catch((error) => {
        this.postError(error);
      });
  }

  private async renderViewportUpdate(frameVersion: number) {
    const handledViewportVersion = this.viewportVersion;
    try {
      if (
        this.isRenderInterrupted(frameVersion) ||
        this.latestPages.length === 0
      ) {
        return;
      }

      const session = await this.ensureSessionReady();
      if (this.isRenderInterrupted(frameVersion)) {
        return;
      }

      if (this.isViewportDragging() && !this.isViewportScrolling()) {
        return;
      }

      if (this.isViewportScrolling()) {
        await this.renderScrollVisiblePage(session, frameVersion, handledViewportVersion);
      } else {
        await this.renderProgressiveStages({
          session,
          pages: this.latestPages,
          generation: this.generation,
          kind: "viewport",
          frameBytes: 0,
          frameVersion,
          viewportVersion: handledViewportVersion,
        });
        this.scheduleIdlePrefetch(frameVersion, this.generation, handledViewportVersion);
      }
    } finally {
      this.viewportRenderQueued = false;
      if (
        !this.disposed &&
        (!this.isViewportDragging() || this.isViewportScrolling()) &&
        this.viewportVersion !== handledViewportVersion
      ) {
        this.scheduleViewportRender();
      }
    }
  }

  private async renderScrollVisiblePage(
    session: RenderSession,
    frameVersion: number,
    viewportVersion: number,
  ) {
    if (this.latestPages.length === 0 || this.isRenderInterrupted(frameVersion)) {
      return;
    }

    await this.renderAndPostComplete({
      session,
      pages: this.selectStagePages(this.latestPages, scrollRenderScreens, { windowed: true }),
      generation: this.generation,
      kind: "scroll",
      frameBytes: 0,
      phase: "scroll-visible",
      layer: "full",
      quality: "full",
      useCacheKey: true,
      updateCacheKey: true,
      collectInteractions: false,
      prioritizeViewport: false,
      cancelOnViewportChange: true,
      pauseWhenDragging: false,
      viewportVersion,
      frameVersion,
      flushResults: true,
    });
  }

  private scheduleIdlePrefetch(frameVersion: number, generation: number, viewportVersion: number) {
    this.clearIdlePrefetchTimer();
    if (
      this.disposed ||
      this.isViewportInteractive() ||
      this.latestPages.length === 0 ||
      generation !== this.generation ||
      frameVersion !== this.documentVersion ||
      viewportVersion !== this.viewportVersion
    ) {
      return;
    }

    this.idlePrefetchTimer = setTimeout(() => {
      this.idlePrefetchTimer = 0;
      if (
        this.disposed ||
        this.isViewportInteractive() ||
        generation !== this.generation ||
        frameVersion !== this.documentVersion ||
        viewportVersion !== this.viewportVersion
      ) {
        return;
      }

      this.renderQueue = this.renderQueue
        .then(() => this.renderIdlePrefetch(frameVersion, generation, viewportVersion))
        .catch((error) => {
          this.postError(error);
        });
    }, idlePrefetchDelayMs) as unknown as number;
  }

  private clearIdlePrefetchTimer() {
    if (!this.idlePrefetchTimer) {
      return;
    }
    clearTimeout(this.idlePrefetchTimer);
    this.idlePrefetchTimer = 0;
  }

  private async renderIdlePrefetch(
    frameVersion: number,
    generation: number,
    viewportVersion: number,
  ) {
    if (
      this.shouldStopRender({
        frameVersion,
        viewportVersion,
        cancelOnViewportChange: true,
        pauseWhenDragging: true,
      }) ||
      generation !== this.generation ||
      this.latestPages.length === 0
    ) {
      return;
    }

    const session = await this.ensureSessionReady();
    if (
      this.shouldStopRender({
        frameVersion,
        viewportVersion,
        cancelOnViewportChange: true,
        pauseWhenDragging: true,
      }) ||
      generation !== this.generation
    ) {
      return;
    }

    await this.renderAndPostComplete({
      session,
      pages: this.selectStagePages(this.latestPages, prefetchRenderScreens, { windowed: false }),
      generation,
      kind: "prefetch",
      frameBytes: 0,
      phase: "prefetch",
      layer: "full",
      quality: "full",
      useCacheKey: true,
      updateCacheKey: true,
      collectInteractions: false,
      prioritizeViewport: true,
      cancelOnViewportChange: true,
      pauseWhenDragging: true,
      viewportVersion,
      frameVersion,
    });
  }

  acceptCanvases(
    generation: number,
    canvases: Array<{
      index: number;
      layer?: CanvasLayer;
      canvas: OffscreenCanvas;
      widthPx: number;
      heightPx: number;
    }>,
    ack?: CanvasAck,
  ) {
    for (const page of canvases) {
      const layer = page.layer || "full";
      this.pageCanvases.set(page.index, {
        canvas: page.canvas,
        widthPx: page.widthPx,
        heightPx: page.heightPx,
      });
      this.pageCacheKeys.delete(page.index);
    }
    this.resolveCanvasAck(generation, ack);
  }

  rejectCanvases(generation: number, error: Error) {
    this.rejectCanvasAck(generation, error);
  }

  async hitBound(request: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
  }) {
    this.renderQueue = this.renderQueue
      .then(() => this.hitBoundExclusive(request))
      .catch((error) => {
        this.postError(error);
      });
    await this.renderQueue;
  }

  async requestInteractions(request: {
    generation: number;
    pageIndices: number[];
  }) {
    const viewportVersion = this.viewportVersion;
    this.renderQueue = this.renderQueue
      .then(() => this.requestInteractionsExclusive({ ...request, viewportVersion }))
      .catch((error) => {
        this.postError(error);
      });
    await this.renderQueue;
  }

  hitText(request: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    rect?: PageRect;
  }) {
    try {
      this.hitTextExclusive(request);
    } catch (error) {
      this.postError(error);
    }
  }

  private hitTextExclusive(request: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    rect?: PageRect;
  }) {
    if (request.generation !== this.generation) {
      return;
    }

    const result = this.hitRenderedTextBox(request.pageIndex, request.x, request.y, request.rect);
    if (request.generation !== this.generation) {
      return;
    }

    this.post({
      type: "text-hit",
      requestId: request.requestId,
      generation: request.generation,
      pageIndex: request.pageIndex,
      x: request.x,
      y: request.y,
      hit: result.hit,
      rect: result.rect,
    });
  }

  resolveTextRect(request: {
    requestId: number;
    generation: number;
    pageIndex: number;
    textId: number;
    rect: PageRect;
  }) {
    try {
      if (request.generation !== this.generation) {
        return;
      }

      const rect = this.resolveRenderedTextRect(request.pageIndex, request.rect);
      if (request.generation !== this.generation) {
        return;
      }

      this.post({
        type: "text-rect",
        requestId: request.requestId,
        generation: request.generation,
        pageIndex: request.pageIndex,
        textId: request.textId,
        rect,
      });
    } catch (error) {
      this.postError(error);
    }
  }

  private async hitBoundExclusive(request: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
  }) {
    try {
      if (request.generation !== this.generation) {
        return;
      }

      const session = await this.ensureSessionReady();
      if (request.generation !== this.generation) {
        return;
      }

      const bound = this.hitCanvasBound(session, request.pageIndex, request.x, request.y);
      if (request.generation !== this.generation) {
        return;
      }

      this.post({
        type: "bound-hit",
        requestId: request.requestId,
        generation: request.generation,
        pageIndex: request.pageIndex,
        x: request.x,
        y: request.y,
        bound,
      });
    } catch (error) {
      this.postError(error);
    }
  }

  private hitRenderedTextBox(pageIndex: number, x: number, y: number, rect?: PageRect) {
    const entry = this.pageCanvases.get(pageIndex);
    const context = entry?.context;
    const pixelPerPt = this.pagePixelPerPt.get(pageIndex);
    if (!entry || !context || !pixelPerPt) {
      return { hit: false };
    }

    const px = Math.floor(x * pixelPerPt);
    const py = Math.floor(y * pixelPerPt);
    if (rect && usesTextRunBounds(rect)) {
      const textRect = this.textRunInkBounds(context, entry, pixelPerPt, rect, py);
      return textRect ? { hit: true, rect: textRect } : { hit: false };
    }

    const start = this.nearestInkPixel(context, entry, px, py, pixelPerPt, rect);
    if (!start) {
      return { hit: false };
    }

    return {
      hit: true,
      rect: this.connectedInkBounds(context, entry, start.x, start.y, pixelPerPt),
    };
  }

  private resolveRenderedTextRect(pageIndex: number, rect: PageRect) {
    const entry = this.pageCanvases.get(pageIndex);
    const context = entry?.context;
    const pixelPerPt = this.pagePixelPerPt.get(pageIndex);
    if (!entry || !context || !pixelPerPt) {
      return undefined;
    }
    return this.textRunInkBounds(context, entry, pixelPerPt, rect);
  }

  private textRunInkBounds(
    context: OffscreenCanvasRenderingContext2D,
    entry: CanvasEntry,
    pixelPerPt: number,
    rect: PageRect,
    preferredY?: number,
  ) {
    const rectLeft = Math.floor(rect.x * pixelPerPt);
    const rectTop = rect.y * pixelPerPt;
    const rectRight = Math.ceil((rect.x + rect.width) * pixelPerPt) - 1;
    const rectBottom = (rect.y + rect.height) * pixelPerPt;
    const rectHeight = Math.max(1, rectBottom - rectTop);
    const yPad = Math.trunc(clamp(rectHeight * 1.6, 4, 48));
    const left = Math.trunc(clamp(rectLeft, 0, Math.max(0, entry.widthPx - 1)));
    const right = Math.trunc(clamp(rectRight, 0, Math.max(0, entry.widthPx - 1)));
    const top = Math.trunc(clamp(Math.floor(rectTop) - yPad, 0, Math.max(0, entry.heightPx - 1)));
    const bottom = Math.trunc(
      clamp(Math.ceil(rectBottom) + yPad, 0, Math.max(0, entry.heightPx - 1)),
    );
    const width = right - left + 1;
    const height = bottom - top + 1;
    if (width <= 0 || height <= 0) {
      return undefined;
    }

    const pixels = context.getImageData(left, top, width, height).data;
    const rowClusters = this.textInkRowClusters(pixels, width, height, top);
    const cluster = chooseTextRowCluster(rowClusters, rectTop, rectBottom, preferredY);
    if (!cluster) {
      return undefined;
    }

    let minX = Number.POSITIVE_INFINITY;
    let maxX = Number.NEGATIVE_INFINITY;
    let minY = Number.POSITIVE_INFINITY;
    let maxY = Number.NEGATIVE_INFINITY;
    for (let y = cluster.top; y <= cluster.bottom; y += 1) {
      const row = y - top;
      for (let x = 0; x < width; x += 1) {
        const index = row * width + x;
        if (!pixelHasVisibleInk(pixels, index * 4)) {
          continue;
        }
        const px = left + x;
        minX = Math.min(minX, px);
        maxX = Math.max(maxX, px);
        minY = Math.min(minY, y);
        maxY = Math.max(maxY, y);
      }
    }

    if (!Number.isFinite(minX)) {
      return undefined;
    }

    return {
      x: minX / pixelPerPt,
      y: minY / pixelPerPt,
      width: (maxX - minX + 1) / pixelPerPt,
      height: (maxY - minY + 1) / pixelPerPt,
    };
  }

  private textInkRowClusters(
    pixels: Uint8ClampedArray,
    width: number,
    height: number,
    top: number,
  ) {
    const clusters: Array<{ top: number; bottom: number; ink: number }> = [];
    for (let y = 0; y < height; y += 1) {
      let rowInk = 0;
      for (let x = 0; x < width; x += 1) {
        const index = y * width + x;
        if (pixelHasVisibleInk(pixels, index * 4)) {
          rowInk += 1;
        }
      }
      if (rowInk === 0) {
        continue;
      }
      const pageY = top + y;
      const last = clusters[clusters.length - 1];
      if (!last || pageY > last.bottom + 1) {
        clusters.push({ top: pageY, bottom: pageY, ink: rowInk });
        continue;
      }
      last.bottom = pageY;
      last.ink += rowInk;
    }
    return clusters;
  }

  private nearestInkPixel(
    context: OffscreenCanvasRenderingContext2D,
    entry: CanvasEntry,
    px: number,
    py: number,
    pixelPerPt: number,
    rect?: PageRect,
  ) {
    let left: number;
    let top: number;
    let right: number;
    let bottom: number;

    if (rect) {
      const rectLeft = Math.floor(rect.x * pixelPerPt);
      const rectTop = Math.floor(rect.y * pixelPerPt);
      const rectRight = Math.ceil((rect.x + rect.width) * pixelPerPt);
      const rectBottom = Math.ceil((rect.y + rect.height) * pixelPerPt);
      const pad = Math.trunc(
        clamp(Math.max(rectRight - rectLeft, rectBottom - rectTop) * 2, 16, 96),
      );
      left = Math.trunc(clamp(rectLeft - pad, 0, Math.max(0, entry.widthPx - 1)));
      top = Math.trunc(clamp(rectTop - pad, 0, Math.max(0, entry.heightPx - 1)));
      right = Math.trunc(clamp(rectRight + pad, 0, Math.max(0, entry.widthPx - 1)));
      bottom = Math.trunc(clamp(rectBottom + pad, 0, Math.max(0, entry.heightPx - 1)));
    } else {
      const radius = 48;
      left = Math.trunc(clamp(px - radius, 0, Math.max(0, entry.widthPx - 1)));
      top = Math.trunc(clamp(py - radius, 0, Math.max(0, entry.heightPx - 1)));
      right = Math.trunc(clamp(px + radius, 0, Math.max(0, entry.widthPx - 1)));
      bottom = Math.trunc(clamp(py + radius, 0, Math.max(0, entry.heightPx - 1)));
    }

    const width = right - left + 1;
    const height = bottom - top + 1;
    if (width <= 0 || height <= 0) {
      return undefined;
    }

    const pixels = context.getImageData(left, top, width, height).data;
    let bestIndex = -1;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (let index = 0; index < width * height; index += 1) {
      if (!pixelHasStrongInk(pixels, index * 4)) {
        continue;
      }
      const x = left + (index % width);
      const y = top + Math.floor(index / width);
      const distance = (x - px) ** 2 + (y - py) ** 2;
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }

    if (bestIndex < 0) {
      return undefined;
    }

    return {
      x: left + (bestIndex % width),
      y: top + Math.floor(bestIndex / width),
    };
  }

  private connectedInkBounds(
    context: OffscreenCanvasRenderingContext2D,
    entry: CanvasEntry,
    px: number,
    py: number,
    pixelPerPt: number,
  ) {
    const radius = 80;
    const left = Math.trunc(clamp(px - radius, 0, Math.max(0, entry.widthPx - 1)));
    const top = Math.trunc(clamp(py - radius, 0, Math.max(0, entry.heightPx - 1)));
    const right = Math.trunc(clamp(px + radius, 0, Math.max(0, entry.widthPx - 1)));
    const bottom = Math.trunc(clamp(py + radius, 0, Math.max(0, entry.heightPx - 1)));
    const width = right - left + 1;
    const height = bottom - top + 1;
    const pixels = context.getImageData(left, top, width, height).data;
    const startX = px - left;
    const startY = py - top;
    const start = startY * width + startX;
    const visited = new Uint8Array(width * height);
    const stack = [start];
    let minX = startX;
    let maxX = startX;
    let minY = startY;
    let maxY = startY;
    let visitedInk = 0;

    while (stack.length > 0 && visitedInk < 16_384) {
      const index = stack.pop()!;
      if (visited[index]) {
        continue;
      }
      visited[index] = 1;
      if (!pixelHasVisibleInk(pixels, index * 4)) {
        continue;
      }

      visitedInk += 1;
      const x = index % width;
      const y = Math.floor(index / width);
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
      minY = Math.min(minY, y);
      maxY = Math.max(maxY, y);

      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if (dx === 0 && dy === 0) {
            continue;
          }
          const nx = x + dx;
          const ny = y + dy;
          if (nx < 0 || ny < 0 || nx >= width || ny >= height) {
            continue;
          }
          const next = ny * width + nx;
          if (!visited[next]) {
            stack.push(next);
          }
        }
      }
    }

    const rectLeft = Math.max(0, left + minX);
    const rectTop = Math.max(0, top + minY);
    const rectRight = Math.min(entry.widthPx - 1, left + maxX);
    const rectBottom = Math.min(entry.heightPx - 1, top + maxY);
    return {
      x: rectLeft / pixelPerPt,
      y: rectTop / pixelPerPt,
      width: (rectRight - rectLeft + 1) / pixelPerPt,
      height: (rectBottom - rectTop + 1) / pixelPerPt,
    };
  }

  resetDocumentState() {
    this.initializedDocument = false;
    this.generation = 0;
    this.documentVersion += 1;
    this.viewportVersion += 1;
    this.viewportRenderQueued = false;
    this.clearIdlePrefetchTimer();
    this.latestPages = [];
    this.pageCanvases.clear();
    this.pageCanvasLru.clear();
    this.pageCacheKeys.clear();
    this.pageLatestCacheKeys.clear();
    this.pagePixelPerPt.clear();
    this.pageInteractionCacheKeys.clear();
    this.pageInteractions.clear();
    this.latestPageLayouts = [];
    for (const ack of this.pendingCanvasAcks.values()) {
      clearTimeout(ack.timeout);
      ack.resolve(null);
    }
    this.pendingCanvasAcks.clear();
  }

  dispose() {
    this.disposed = true;
    this.resetDocumentState();
    this.releaseSession?.();
    this.sessionReady = undefined;
  }

  private hitCanvasBound(
    session: RenderSession,
    pageIndex: number,
    x: number,
    y: number,
  ): BoundInteraction | undefined {
    const hitCanvasPageBound = (session as any).hitCanvasPageBound;
    if (typeof hitCanvasPageBound !== "function") {
      return undefined;
    }

    const bound = hitCanvasPageBound.call(session, {
      pageOffset: pageIndex,
      x,
      y,
    });
    if (!bound?.rect) {
      return undefined;
    }

    return {
      id: 0,
      kind: typeof bound.kind === "string" ? bound.kind : "bound",
      rect: bound.rect,
    };
  }

  private async requestInteractionsExclusive(request: {
    generation: number;
    pageIndices: number[];
    viewportVersion: number;
  }) {
    if (
      request.generation !== this.generation ||
      request.viewportVersion !== this.viewportVersion ||
      this.isViewportInteractive()
    ) {
      return;
    }

    const session = await this.ensureSessionReady();
    if (
      request.generation !== this.generation ||
      request.viewportVersion !== this.viewportVersion ||
      this.isViewportInteractive()
    ) {
      return;
    }

    const interactions: PageInteractions[] = [];
    for (const pageIndex of request.pageIndices) {
      if (
        request.generation !== this.generation ||
        request.viewportVersion !== this.viewportVersion ||
        this.isViewportInteractive()
      ) {
        return;
      }

      const pixelPerPt = this.pagePixelPerPt.get(pageIndex);
      if (!pixelPerPt) {
        continue;
      }

      interactions.push(await this.renderPageInteractions(session, pageIndex, pixelPerPt));
    }

    if (
      request.generation !== this.generation ||
      request.viewportVersion !== this.viewportVersion ||
      this.isViewportInteractive() ||
      interactions.length === 0
    ) {
      return;
    }

    this.post({
      type: "interactions",
      generation: request.generation,
      interactions,
    });
  }

  private computePagePixelPerPt(index: number, width: number, height: number) {
    const devicePixelRatio = clamp(this.config.viewport?.devicePixelRatio || 1, 1, 4);
    const layout = this.latestPageLayouts.find((candidate) => candidate.index === index);
    if (layout?.width && layout.width > 0) {
      return clampPixelPerPt((layout.width * devicePixelRatio) / Math.max(width, 1));
    }

    const viewport = this.config.viewport || {};
    const viewportWidth = Math.max(1, viewport.width || viewport.window?.innerWidth || width);
    const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || height);
    const fitWidth = viewportWidth / Math.max(width, 1);
    const fitHeight = viewportHeight / Math.max(height, 1);
    const scale =
      this.config.previewMode === "Slide"
        ? Math.max(0.1, Math.min(fitWidth, fitHeight))
        : Math.max(0.1, fitWidth);
    const cssWidth = Math.ceil(width * scale);
    return clampPixelPerPt((cssWidth * devicePixelRatio) / Math.max(width, 1));
  }

  private async initializeRenderer(wasmUrl: string): Promise<RenderSession> {
    const startedAt = performance.now();
    this.postStatus("initializing-renderer", "loading typst.ts renderer");
    this.renderer = createTypstRenderer();
    await this.renderer.init({
      getWrapper: () => Promise.resolve(rendererWrapper),
      getModule: () => wasmUrl,
    });

    try {
      this.post({
        type: "renderer-ready",
        elapsedMs: performance.now() - startedAt,
        buildInfo: rendererWrapper.renderer_build_info(),
      });
    } catch (_error) {
      this.post({ type: "renderer-ready", elapsedMs: performance.now() - startedAt });
    }

    return new Promise<RenderSession>((resolveSession, rejectSession) => {
      let sessionResolved = false;
      let releaseResolve: () => void = () => {};
      const releasePromise = new Promise<void>((resolve) => {
        releaseResolve = resolve;
      });
      this.releaseSession = releaseResolve;

      this.renderer!.runWithSession(async (session) => {
        sessionResolved = true;
        resolveSession(session);
        await releasePromise;
      }).catch((error) => {
        if (!sessionResolved) {
          rejectSession(error);
        } else {
          this.postError(error);
        }
      });
    });
  }

  private async renderDocumentFrame(
    kind: string,
    payload: Uint8Array,
    frameBytes: number,
    frameVersion: number,
  ) {
    const session = await this.ensureSessionReady();
    if (!this.renderer) {
      throw new Error("renderer is not initialized");
    }

    if (kind === "new" || !this.initializedDocument) {
      session.reset();
      this.pageCacheKeys.clear();
      this.pageInteractionCacheKeys.clear();
      this.pageInteractions.clear();
      this.initializedDocument = true;
    }
    session.manipulateData({ action: "merge", data: payload });
    const pages = session.retrievePagesInfo().map((page, index) => {
      const pixelPerPt = this.computePagePixelPerPt(index, page.width, page.height);
      this.pagePixelPerPt.set(index, pixelPerPt);
      return {
        index,
        width: page.width,
        height: page.height,
        pixelPerPt,
      };
    });
    this.latestPages = pages;
    const nextGeneration = ++this.generation;
    const canvasAck = await this.ensureCanvases(nextGeneration, pages);
    if (canvasAck?.layouts) {
      this.latestPageLayouts = canvasAck.layouts;
    }
    await this.renderProgressiveStages({
      session,
      pages,
      generation: nextGeneration,
      kind,
      frameBytes,
      frameVersion,
      viewportVersion: this.viewportVersion,
    });
    this.scheduleIdlePrefetch(frameVersion, nextGeneration, this.viewportVersion);
  }

  private async renderProgressiveStages(options: {
    session: RenderSession;
    pages: PageSpec[];
    generation: number;
    kind: string;
    frameBytes: number;
    frameVersion: number;
    viewportVersion: number;
  }) {
    const stages = [
      {
        phase: "visible",
        layer: "full" as const,
        quality: "full" as const,
        screens: visibleRenderScreens,
        useCacheKey: true,
        updateCacheKey: true,
        collectInteractions: true,
        prioritizeViewport: true,
        cancelOnViewportChange: true,
        pauseWhenDragging: true,
      },
    ];

    for (const stage of stages) {
      if (this.isRenderInterrupted(options.frameVersion)) {
        return;
      }

      await this.renderAndPostComplete({
        ...options,
        pages: this.selectStagePages(options.pages, stage.screens, { windowed: false }),
        phase: stage.phase,
        layer: stage.layer,
        quality: stage.quality,
        useCacheKey: stage.useCacheKey,
        updateCacheKey: stage.updateCacheKey,
        collectInteractions: stage.collectInteractions,
        prioritizeViewport: stage.prioritizeViewport,
        cancelOnViewportChange: stage.cancelOnViewportChange,
        pauseWhenDragging: stage.pauseWhenDragging,
        viewportVersion: options.viewportVersion,
      });
    }
  }

  private async renderAndPostComplete(options: RenderAndPostOptions) {
    if (this.shouldStopRender(options)) {
      return;
    }

    const results = await this.renderPages(options.session, options.pages, {
      useCacheKey: options.useCacheKey,
      updateCacheKey: options.updateCacheKey,
      collectInteractions: options.collectInteractions,
      prioritizeViewport: options.prioritizeViewport,
      cancelOnViewportChange: options.cancelOnViewportChange,
      pauseWhenDragging: options.pauseWhenDragging,
      viewportVersion: options.viewportVersion,
      frameVersion: options.frameVersion,
      layer: options.layer,
      quality: options.quality,
      generation: options.generation,
      onResult: options.flushResults
        ? (result) => this.postRenderComplete(options, [result])
        : undefined,
    });
    if (this.isRenderInterrupted(options.frameVersion)) {
      return;
    }
    if (results.length === 0 && this.shouldStopRender(options)) {
      return;
    }
    this.enforceCanvasBuffer(options.generation);

    if (options.flushResults) {
      return;
    }

    this.postRenderComplete(options, results);
  }

  private postRenderComplete(options: RenderAndPostOptions, results: PageRenderResult[]) {
    this.post({
      type: "render-complete",
      generation: options.generation,
      kind: options.kind,
      frameBytes: options.frameBytes,
      phase: options.phase,
      layer: options.layer,
      quality: options.quality,
      pageCount: options.pages.length,
      renderedPages: results.length,
      pageIndices: results.map((result) => result.pageIndex),
      fullPageIndices: results
        .filter((result) => result.fullPage)
        .map((result) => result.pageIndex),
      interactions: results.flatMap((result) => result.interactions || []),
      invalidatedInteractions: results.flatMap((result) => result.invalidatedInteractions || []),
    });
  }

  private ensureCanvases(nextGeneration: number, pages: PageSpec[]): Promise<CanvasAck | null> {
    const livePages = new Set(pages.map((page) => page.index));
    for (const index of this.pageCanvases.keys()) {
      if (!livePages.has(index)) {
        this.pageCanvases.delete(index);
        this.pageCanvasLru.delete(index);
        this.pageCacheKeys.delete(index);
        this.pageLatestCacheKeys.delete(index);
        this.pagePixelPerPt.delete(index);
        this.pageInteractionCacheKeys.delete(index);
        this.pageInteractions.delete(index);
      }
    }

    this.post({
      type: "ensure-pages",
      generation: nextGeneration,
      pages,
    });

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.rejectCanvasAck(
          nextGeneration,
          new Error(`timed out waiting for offscreen canvases for generation ${nextGeneration}`),
        );
      }, 30_000) as unknown as number;
      this.pendingCanvasAcks.set(nextGeneration, {
        resolve,
        reject,
        timeout,
      });
    });
  }

  private async renderPages(
    session: RenderSession,
    pages: PageRenderSpec[],
    options: RenderPagesOptions,
  ) {
    const results: PageRenderResult[] = [];
    const pushResult = (result: PageRenderResult) => {
      results.push(result);
      options.onResult?.(result);
    };
    let batchRendered = 0;
    const pendingPages = options.prioritizeViewport ? [...pages] : undefined;
    for (let pageIndex = 0; pageIndex < pages.length; pageIndex += 1) {
      if (this.shouldStopRender(options)) {
        break;
      }

      const page = pendingPages ? this.takeNextViewportPage(pendingPages) : pages[pageIndex];
      if (!page) {
        break;
      }

      const entry = this.pageCanvases.get(page.index);
      if (!entry) {
        throw new Error(`missing ${options.layer} offscreen canvas for page ${page.index}`);
      }

      const pixelPerPt =
        options.quality === "preview"
          ? computeInteractivePixelPerPt(page.pixelPerPt)
          : page.pixelPerPt;
      const windowKey = renderWindowKey(page);
      if (
        options.quality === "full" &&
        entry.hasContent &&
        entry.quality === "full" &&
        entry.renderedGeneration === options.generation &&
        entry.renderedPixelPerPt === pixelPerPt &&
        (entry.renderedWindowKey === "full" || entry.renderedWindowKey === windowKey)
      ) {
        this.touchCanvasLru(page.index);
        pushResult({
          pageIndex: page.index,
          quality: entry.quality,
          fullPage: entry.renderedWindowKey === "full",
        });
        continue;
      }

      if (
        options.quality === "preview" &&
        entry.hasContent &&
        entry.renderedGeneration === options.generation &&
        (entry.quality === "full" ||
          (entry.quality === "preview" &&
            entry.renderedPixelPerPt === pixelPerPt &&
            entry.renderedWindowKey === windowKey))
      ) {
        this.touchCanvasLru(page.index);
        pushResult({ pageIndex: page.index, quality: entry.quality });
        continue;
      }

      const widthPx = Math.max(1, Math.ceil(page.width * pixelPerPt));
      const heightPx = Math.max(1, Math.ceil(page.height * pixelPerPt));
      let resized = false;
      if (entry.canvas.width !== widthPx) {
        entry.canvas.width = widthPx;
        entry.hasContent = false;
        this.pageCacheKeys.delete(page.index);
        resized = true;
      }
      if (entry.canvas.height !== heightPx) {
        entry.canvas.height = heightPx;
        entry.hasContent = false;
        this.pageCacheKeys.delete(page.index);
        resized = true;
      }
      entry.widthPx = widthPx;
      entry.heightPx = heightPx;

      const context =
        entry.context ||
        entry.canvas.getContext("2d", {
          alpha: false,
          desynchronized: true,
        });
      if (!context) {
        throw new Error(`cannot create 2d context for page ${page.index}`);
      }
      entry.context = context;
      const fullPageRender = isFullPageRender(page);
      const fullPageResult = options.quality === "full" && fullPageRender;
      if (!fullPageRender && (resized || !entry.hasContent)) {
        this.initializeTargetCanvas(context, entry);
      }
      const cacheKey =
        options.useCacheKey && fullPageRender && entry.hasContent && entry.quality === "full"
          ? this.pageCacheKeys.get(page.index)
          : undefined;
      const renderContext = fullPageRender
        ? context
        : this.ensureScratchContext(entry, page.index, options.layer);

      const renderOptions = {
        canvas: renderContext,
        pageOffset: page.index,
        backgroundColor: "#ffffff",
        pixelPerPt,
        window: page.window,
        cacheKey,
        dataSelection: {
          body: true,
        },
      } as any;
      const result = await session.renderCanvas(renderOptions);
      if (this.isRenderInterrupted(options.frameVersion)) {
        if (!fullPageRender) {
          this.releaseScratch(entry);
        }
        break;
      }
      const stopAfterCommit = this.shouldStopRender(options);

      const cacheHit = !!cacheKey && result.cacheKey === cacheKey;
      if (!cacheHit && fullPageRender) {
        entry.hasContent = true;
      } else if (!cacheHit) {
        this.commitScratchToTarget(entry, context, renderContext, page, pixelPerPt);
        entry.hasContent = true;
      }
      entry.quality = options.quality;
      entry.renderedGeneration = options.generation;
      entry.renderedPixelPerPt = pixelPerPt;
      entry.renderedWindowKey = windowKey;
      if (!fullPageRender) {
        this.releaseScratch(entry);
      }
      if (entry.hasContent) {
        this.touchCanvasLru(page.index);
      }

      if (options.quality === "full" && result.cacheKey) {
        this.pageLatestCacheKeys.set(page.index, result.cacheKey);
      }
      if (options.quality === "full" && options.updateCacheKey && fullPageRender && result.cacheKey) {
        this.pageCacheKeys.set(page.index, result.cacheKey);
      }

      if (stopAfterCommit) {
        pushResult({ pageIndex: page.index, quality: options.quality, fullPage: fullPageResult });
      } else if (options.quality === "full" && options.collectInteractions) {
        pushResult({
          pageIndex: page.index,
          quality: options.quality,
          fullPage: fullPageResult,
          interactions: await this.renderPageInteractions(session, page.index, pixelPerPt),
        });
      } else if (
        options.quality === "full" &&
        result.cacheKey &&
        this.pageInteractionCacheKeys.get(page.index) !== result.cacheKey
      ) {
        const hadInteractions =
          this.pageInteractionCacheKeys.has(page.index) || this.pageInteractions.has(page.index);
        this.pageInteractionCacheKeys.delete(page.index);
        this.pageInteractions.delete(page.index);
        pushResult(
          hadInteractions
            ? {
                pageIndex: page.index,
                quality: options.quality,
                fullPage: fullPageResult,
                invalidatedInteractions: [page.index],
              }
            : { pageIndex: page.index, quality: options.quality, fullPage: fullPageResult },
        );
      } else {
        pushResult({ pageIndex: page.index, quality: options.quality, fullPage: fullPageResult });
      }

      batchRendered += 1;
      if (batchRendered >= 8) {
        batchRendered = 0;
        await yieldToEventLoop();
      }
      if (stopAfterCommit) {
        break;
      }
    }
    return results;
  }

  private touchCanvasLru(pageIndex: number) {
    this.pageCanvasLru.delete(pageIndex);
    this.pageCanvasLru.set(pageIndex, true);
  }

  private enforceCanvasBuffer(generation: number) {
    if (this.pageCanvasLru.size <= maxCanvasBufferPages) {
      return;
    }

    const protectedPages = new Set(
      this.selectStagePages(this.latestPages, prefetchRenderScreens, { windowed: false }).map(
        (page) => page.index,
      ),
    );
    const evicted: number[] = [];
    while (this.pageCanvasLru.size > maxCanvasBufferPages) {
      const candidate = [...this.pageCanvasLru.keys()].find((pageIndex) => !protectedPages.has(pageIndex));
      if (candidate === undefined) {
        break;
      }

      const entry = this.pageCanvases.get(candidate);
      if (entry) {
        this.evictCanvasEntry(entry);
      }
      this.pageCanvasLru.delete(candidate);
      this.pageCacheKeys.delete(candidate);
      this.pageLatestCacheKeys.delete(candidate);
      this.pageInteractionCacheKeys.delete(candidate);
      this.pageInteractions.delete(candidate);
      evicted.push(candidate);
    }

    if (evicted.length > 0) {
      this.post({
        type: "render-evicted",
        generation,
        pageIndices: evicted,
      });
    }
  }

  private evictCanvasEntry(entry: CanvasEntry) {
    entry.canvas.width = 1;
    entry.canvas.height = 1;
    entry.widthPx = 1;
    entry.heightPx = 1;
    entry.hasContent = false;
    entry.quality = undefined;
    entry.renderedGeneration = undefined;
    entry.renderedPixelPerPt = undefined;
    entry.renderedWindowKey = undefined;
    entry.context = undefined;
    this.releaseScratch(entry);
  }

  private takeNextViewportPage(pages: PageRenderSpec[]) {
    if (pages.length === 0) {
      return undefined;
    }

    let bestIndex = 0;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (let index = 0; index < pages.length; index += 1) {
      const distance = this.pageDistanceToCurrentViewport(pages[index]);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }

    return pages.splice(bestIndex, 1)[0];
  }

  private ensureScratchContext(entry: CanvasEntry, pageIndex: number, layer: CanvasLayer) {
    if (!entry.scratch) {
      entry.scratch = new OffscreenCanvas(entry.widthPx, entry.heightPx);
    }
    if (entry.scratch.width !== entry.widthPx) {
      entry.scratch.width = entry.widthPx;
    }
    if (entry.scratch.height !== entry.heightPx) {
      entry.scratch.height = entry.heightPx;
    }

    const context =
      entry.scratchContext ||
      entry.scratch.getContext("2d", {
        alpha: false,
        desynchronized: true,
      });
    if (!context) {
      throw new Error(`cannot create ${layer} scratch context for page ${pageIndex}`);
    }
    entry.scratchContext = context;
    return context;
  }

  private releaseScratch(entry: CanvasEntry) {
    if (!entry.scratch) {
      return;
    }
    if (entry.scratch.width !== 1) {
      entry.scratch.width = 1;
    }
    if (entry.scratch.height !== 1) {
      entry.scratch.height = 1;
    }
    entry.scratchContext = undefined;
  }

  private initializeTargetCanvas(
    target: OffscreenCanvasRenderingContext2D,
    entry: CanvasEntry,
  ) {
    target.setTransform(1, 0, 0, 1, 0, 0);
    target.fillStyle = "#ffffff";
    target.fillRect(0, 0, entry.widthPx, entry.heightPx);
  }

  private commitScratchToTarget(
    entry: CanvasEntry,
    target: OffscreenCanvasRenderingContext2D,
    scratch: OffscreenCanvasRenderingContext2D,
    page: PageRenderSpec,
    pixelPerPt: number,
  ) {
    target.setTransform(1, 0, 0, 1, 0, 0);
    const rect = commitPixelRect(page, pixelPerPt, entry.widthPx, entry.heightPx);
    if (!rect) {
      return;
    }
    target.clearRect(rect.x, rect.y, rect.width, rect.height);
    target.drawImage(
      scratch.canvas,
      rect.x,
      rect.y,
      rect.width,
      rect.height,
      rect.x,
      rect.y,
      rect.width,
      rect.height,
    );
  }

  private pageDistanceToCurrentViewport(page: PageRenderSpec) {
    const viewport = this.config.viewport || {};
    const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || 1);
    const viewportTop = Math.max(0, viewport.scrollTop || 0);
    const viewportBottom = viewportTop + viewportHeight;
    const viewportCenter = (viewportTop + viewportBottom) / 2;
    const layout = this.latestPageLayouts.find((candidate) => candidate.index === page.index);
    if (!layout) {
      return page.index * 1_000_000;
    }
    if (page.window) {
      const scale = layout.scale || layout.height / Math.max(page.height, 1) || 1;
      const top = layout.top + page.window.lo.y * scale;
      const bottom = layout.top + page.window.hi.y * scale;
      return distanceToRange(top, bottom, viewportCenter, viewportTop, viewportBottom);
    }
    return distanceToViewport(layout, viewportCenter, viewportTop, viewportBottom);
  }

  private async renderPageInteractions(
    session: RenderSession,
    pageIndex: number,
    pixelPerPt: number,
  ) {
    const cacheKey = this.pageLatestCacheKeys.get(pageIndex) || this.pageCacheKeys.get(pageIndex);
    const cached = this.pageInteractions.get(pageIndex);
    if (cacheKey && cached && this.pageInteractionCacheKeys.get(pageIndex) === cacheKey) {
      return cached;
    }

    const metadata = await session.renderCanvas({
      pageOffset: pageIndex,
      pixelPerPt,
      dataSelection: {
        body: false,
        semantics: true,
      },
    } as any);
    const interactions = parsePageInteractions(pageIndex, metadata.htmlSemantics?.[0] || "");
    if (cacheKey) {
      this.pageInteractionCacheKeys.set(pageIndex, cacheKey);
    }
    this.pageInteractions.set(pageIndex, interactions);
    return interactions;
  }

  private selectStagePages(
    pages: PageSpec[],
    screens: number,
    options: { windowed?: boolean } = {},
  ): PageRenderSpec[] {
    const viewport = this.config.viewport || {};
    const viewportTop = Math.max(0, viewport.scrollTop || 0);
    return this.selectStagePagesAt(pages, screens, viewportTop, options);
  }

  private selectStagePagesAt(
    pages: PageSpec[],
    screens: number,
    viewportTop: number,
    options: { windowed?: boolean } = {},
  ): PageRenderSpec[] {
    if (!Number.isFinite(screens)) {
      return pages.map((page) => ({ ...page }));
    }

    const viewport = this.config.viewport || {};
    const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || 1);
    const viewportBottom = viewportTop + viewportHeight;
    const sideScreens = Math.max(0, (screens - 1) / 2);
    const rangeTop = Math.max(0, viewportTop - viewportHeight * sideScreens);
    const rangeBottom = viewportBottom + viewportHeight * sideScreens;
    const viewportCenter = (viewportTop + viewportBottom) / 2;
    const layouts = new Map(this.latestPageLayouts.map((layout) => [layout.index, layout]));

    return pages
      .flatMap((page) => {
        const layout = layouts.get(page.index);
        if (!layout || layout.bottom < rangeTop || layout.top > rangeBottom) {
          return [];
        }

        const scale = layout.scale || layout.height / Math.max(page.height, 1) || 1;
        if (options.windowed === false) {
          return [
            {
              ...page,
              distance: distanceToViewport(layout, viewportCenter, viewportTop, viewportBottom),
            },
          ];
        }

        const loY = clamp((rangeTop - layout.top) / scale, 0, page.height);
        const hiY = clamp((rangeBottom - layout.top) / scale, 0, page.height);
        if (hiY <= loY) {
          return [];
        }

        return [
          {
            ...page,
            window: {
              lo: { x: -1, y: Math.max(0, loY - 1) },
              hi: { x: page.width + 1, y: Math.min(page.height, hiY + 1) },
            },
            distance: distanceToViewport(layout, viewportCenter, viewportTop, viewportBottom),
          },
        ];
      })
      .sort((a, b) => a.distance - b.distance || a.index - b.index)
      .map(({ distance: _distance, ...page }) => page);
  }

  private isRenderInterrupted(frameVersion: number) {
    return this.disposed || frameVersion !== this.documentVersion;
  }

  private shouldStopRender(options: {
    frameVersion: number;
    cancelOnViewportChange?: boolean;
    pauseWhenDragging?: boolean;
    viewportVersion?: number;
  }) {
    return (
      this.isRenderInterrupted(options.frameVersion) ||
      (!!options.pauseWhenDragging && this.isViewportInteractive()) ||
      (!!options.cancelOnViewportChange && options.viewportVersion !== this.viewportVersion)
    );
  }

  private isViewportDragging() {
    return !!this.config.viewport?.dragging;
  }

  private isViewportScrolling() {
    return !!this.config.viewport?.scrolling;
  }

  private isViewportInteractive() {
    return this.isViewportDragging() || this.isViewportScrolling();
  }

  private resolveCanvasAck(generation: number, canvasAck?: CanvasAck) {
    const pendingAck = this.pendingCanvasAcks.get(generation);
    if (!pendingAck) {
      return;
    }

    this.pendingCanvasAcks.delete(generation);
    clearTimeout(pendingAck.timeout);
    pendingAck.resolve(canvasAck || null);
  }

  private rejectCanvasAck(generation: number, error: Error) {
    const ack = this.pendingCanvasAcks.get(generation);
    if (!ack) {
      this.postError(error);
      return;
    }

    this.pendingCanvasAcks.delete(generation);
    clearTimeout(ack.timeout);
    ack.reject(error);
  }

  private postStatus(state: string, message: string) {
    this.post({ type: "status", state, message });
  }

  private postError(error: unknown) {
    this.post({
      type: "error",
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
    });
  }
}

function yieldToEventLoop(): Promise<void> {
  const scheduler = (globalThis as any).scheduler;
  if (typeof scheduler?.yield === "function") {
    return scheduler.yield();
  }
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function distanceToViewport(
  layout: PageLayout,
  viewportCenter: number,
  viewportTop: number,
  viewportBottom: number,
) {
  return distanceToRange(layout.top, layout.bottom, viewportCenter, viewportTop, viewportBottom);
}

function distanceToRange(
  top: number,
  bottom: number,
  viewportCenter: number,
  viewportTop: number,
  viewportBottom: number,
) {
  if (top <= viewportBottom && bottom >= viewportTop) {
    return Math.abs((top + bottom) / 2 - viewportCenter) * 0.001;
  }
  if (bottom < viewportTop) {
    return viewportTop - bottom;
  }
  return top - viewportBottom;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function isFullPageRender(page: PageRenderSpec) {
  const window = page.window;
  if (!window) {
    return true;
  }
  return (
    window.lo.x <= 0 &&
    window.lo.y <= 0 &&
    window.hi.x >= page.width &&
    window.hi.y >= page.height
  );
}

function commitPixelRect(
  page: PageRenderSpec,
  pixelPerPt: number,
  widthPx: number,
  heightPx: number,
) {
  if (!page.window) {
    return { x: 0, y: 0, width: widthPx, height: heightPx };
  }

  const x = Math.trunc(clamp(Math.floor(page.window.lo.x * pixelPerPt), 0, widthPx));
  const y = Math.trunc(clamp(Math.floor(page.window.lo.y * pixelPerPt), 0, heightPx));
  const right = Math.trunc(clamp(Math.ceil(page.window.hi.x * pixelPerPt), 0, widthPx));
  const bottom = Math.trunc(clamp(Math.ceil(page.window.hi.y * pixelPerPt), 0, heightPx));
  if (right <= x || bottom <= y) {
    return undefined;
  }
  return { x, y, width: right - x, height: bottom - y };
}

function renderWindowKey(page: PageRenderSpec) {
  if (!page.window) {
    return "full";
  }
  const { lo, hi } = page.window;
  return `${lo.x}:${lo.y}:${hi.x}:${hi.y}`;
}

function usesTextRunBounds(rect: PageRect) {
  return rect.width > rect.height * 3;
}

function chooseTextRowCluster(
  clusters: Array<{ top: number; bottom: number; ink: number }>,
  rectTop: number,
  rectBottom: number,
  preferredY?: number,
) {
  let best: { top: number; bottom: number; ink: number } | undefined;
  let bestScore = Number.NEGATIVE_INFINITY;
  const rectCenter = (rectTop + rectBottom) / 2;
  for (const cluster of clusters) {
    const overlap = Math.min(cluster.bottom + 1, rectBottom) - Math.max(cluster.top, rectTop);
    const clusterCenter = (cluster.top + cluster.bottom + 1) / 2;
    const distance = Math.abs(clusterCenter - (preferredY ?? rectCenter));
    const score = Math.max(0, overlap) * 10_000 + cluster.ink - distance;
    if (score > bestScore) {
      bestScore = score;
      best = cluster;
    }
  }
  return best;
}

function pixelDistanceFromWhite(pixels: Uint8ClampedArray, offset: number) {
  return (
    Math.abs(255 - pixels[offset]) +
    Math.abs(255 - pixels[offset + 1]) +
    Math.abs(255 - pixels[offset + 2])
  );
}

function pixelHasStrongInk(pixels: Uint8ClampedArray, offset: number) {
  const alpha = pixels[offset + 3];
  return alpha > 0 && pixelDistanceFromWhite(pixels, offset) > 48;
}

function pixelHasVisibleInk(pixels: Uint8ClampedArray, offset: number) {
  const alpha = pixels[offset + 3];
  return alpha > 0 && pixelDistanceFromWhite(pixels, offset) > 12;
}

function clampPixelPerPt(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return minPixelPerPt;
  }
  const bucketed = Math.ceil((value - 1e-3) * 4) / 4;
  return clamp(bucketed, minPixelPerPt, maxPixelPerPt);
}

function computeInteractivePixelPerPt(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return minInteractivePixelPerPt;
  }
  return clamp(value * interactivePixelPerPtScale, minInteractivePixelPerPt, maxInteractivePixelPerPt);
}
