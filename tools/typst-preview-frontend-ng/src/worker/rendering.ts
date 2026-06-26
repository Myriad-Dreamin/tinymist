import {
  createTypstRenderer,
  type RenderSession,
  type TypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import * as rendererWrapper from "@myriaddreamin/typst-ts-renderer";
import { parsePageInteractions, type BoundInteraction } from "../interactions";
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

const pixelPerPt = 3;

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
  private readonly pageCanvases = new Map<number, CanvasEntry>();
  private readonly pageCacheKeys = new Map<number, string>();
  private readonly pageLatestCacheKeys = new Map<number, string>();
  private readonly pageInteractionCacheKeys = new Map<number, string>();
  private readonly pendingCanvasAcks = new Map<number, PendingCanvasAck>();
  private latestPageLayouts: PageLayout[] = [];

  constructor(private readonly post: WorkerPost) {}

  configure(config: WorkerConfig) {
    this.disposed = false;
    this.config = { ...this.config, ...config };
    this.rendererWasmUrl = config.rendererWasmUrl || this.rendererWasmUrl;
  }

  setViewport(viewport: any, layouts?: PageLayout[]) {
    this.config = { ...this.config, viewport };
    if (layouts) {
      this.latestPageLayouts = layouts;
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

  acceptCanvases(
    generation: number,
    canvases: Array<{ index: number; canvas: OffscreenCanvas; widthPx: number; heightPx: number }>,
    ack?: CanvasAck,
  ) {
    for (const page of canvases) {
      this.pageCanvases.set(page.index, {
        canvas: page.canvas,
        widthPx: page.widthPx,
        heightPx: page.heightPx,
      });
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

  resetDocumentState() {
    this.initializedDocument = false;
    this.generation = 0;
    this.documentVersion += 1;
    this.pageCanvases.clear();
    this.pageCacheKeys.clear();
    this.pageLatestCacheKeys.clear();
    this.pageInteractionCacheKeys.clear();
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
      throw new Error("typst.ts renderer does not provide canvas bound hit testing");
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
      this.initializedDocument = true;
    }
    session.manipulateData({ action: "merge", data: payload });
    const pages = session.retrievePagesInfo().map((page, index) => ({
      index,
      width: page.width,
      height: page.height,
      pixelPerPt,
    }));
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
    });
  }

  private async renderProgressiveStages(options: {
    session: RenderSession;
    pages: PageSpec[];
    generation: number;
    kind: string;
    frameBytes: number;
    frameVersion: number;
  }) {
    const stages = [
      { phase: "near-3", screens: 3, useCacheKey: true, updateCacheKey: false },
      { phase: "near-27", screens: 27, useCacheKey: true, updateCacheKey: false },
      { phase: "all", screens: Number.POSITIVE_INFINITY, useCacheKey: true, updateCacheKey: true },
    ];

    for (const stage of stages) {
      if (this.isRenderInterrupted(options.frameVersion)) {
        return;
      }

      await this.renderAndPostComplete({
        ...options,
        pages: this.selectStagePages(options.pages, stage.screens),
        phase: stage.phase,
        useCacheKey: stage.useCacheKey,
        updateCacheKey: stage.updateCacheKey,
      });
    }
  }

  private async renderAndPostComplete(options: {
    session: RenderSession;
    pages: PageRenderSpec[];
    generation: number;
    kind: string;
    frameBytes: number;
    phase: string;
    useCacheKey: boolean;
    updateCacheKey: boolean;
    frameVersion: number;
  }) {
    if (this.isRenderInterrupted(options.frameVersion)) {
      return;
    }

    const results = await this.renderPages(options.session, options.pages, {
      useCacheKey: options.useCacheKey,
      updateCacheKey: options.updateCacheKey,
      frameVersion: options.frameVersion,
    });
    if (this.isRenderInterrupted(options.frameVersion)) {
      return;
    }

    this.post({
      type: "render-complete",
      generation: options.generation,
      kind: options.kind,
      frameBytes: options.frameBytes,
      phase: options.phase,
      pageCount: options.pages.length,
      renderedPages: results.length,
      interactions: results.flatMap((result) => result.interactions || []),
    });
  }

  private ensureCanvases(nextGeneration: number, pages: PageSpec[]): Promise<CanvasAck | null> {
    const livePages = new Set(pages.map((page) => page.index));
    for (const index of this.pageCanvases.keys()) {
      if (!livePages.has(index)) {
        this.pageCanvases.delete(index);
        this.pageCacheKeys.delete(index);
        this.pageLatestCacheKeys.delete(index);
        this.pageInteractionCacheKeys.delete(index);
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
    options: { useCacheKey: boolean; updateCacheKey: boolean; frameVersion: number },
  ) {
    const results: PageRenderResult[] = [];
    let batchRendered = 0;
    for (const page of pages) {
      if (this.isRenderInterrupted(options.frameVersion)) {
        break;
      }

      const entry = this.pageCanvases.get(page.index);
      if (!entry) {
        throw new Error(`missing offscreen canvas for page ${page.index}`);
      }

      const widthPx = Math.max(1, Math.ceil(page.width * page.pixelPerPt));
      const heightPx = Math.max(1, Math.ceil(page.height * page.pixelPerPt));
      if (entry.canvas.width !== widthPx) {
        entry.canvas.width = widthPx;
      }
      if (entry.canvas.height !== heightPx) {
        entry.canvas.height = heightPx;
      }

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

      const renderOptions = {
        canvas: context,
        pageOffset: page.index,
        backgroundColor: "#ffffff",
        pixelPerPt: page.pixelPerPt,
        window: page.window,
        cacheKey: options.useCacheKey ? this.pageCacheKeys.get(page.index) : undefined,
        dataSelection: {
          body: true,
        },
      } as any;
      const result = await session.renderCanvas(renderOptions);
      if (result.cacheKey) {
        this.pageLatestCacheKeys.set(page.index, result.cacheKey);
      }
      if (options.updateCacheKey && result.cacheKey) {
        this.pageCacheKeys.set(page.index, result.cacheKey);
      }

      if (result.cacheKey && this.pageInteractionCacheKeys.get(page.index) !== result.cacheKey) {
        const metadata = await session.renderCanvas({
          pageOffset: page.index,
          pixelPerPt: page.pixelPerPt,
          dataSelection: {
            body: false,
            semantics: true,
          },
        } as any);
        this.pageInteractionCacheKeys.set(page.index, result.cacheKey);
        results.push({
          interactions: parsePageInteractions(page.index, metadata.htmlSemantics?.[0] || ""),
        });
      } else {
        results.push({});
      }

      batchRendered += 1;
      if (batchRendered >= 8) {
        batchRendered = 0;
        await yieldToEventLoop();
      }
    }
    return results;
  }

  private selectStagePages(pages: PageSpec[], screens: number): PageRenderSpec[] {
    if (!Number.isFinite(screens)) {
      return pages.map((page) => ({ ...page }));
    }

    const viewport = this.config.viewport || {};
    const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || 1);
    const viewportTop = Math.max(0, viewport.scrollTop || 0);
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
  if (layout.top <= viewportBottom && layout.bottom >= viewportTop) {
    return Math.abs((layout.top + layout.bottom) / 2 - viewportCenter) * 0.001;
  }
  if (layout.bottom < viewportTop) {
    return viewportTop - layout.bottom;
  }
  return layout.top - viewportBottom;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
