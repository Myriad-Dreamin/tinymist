import {
  createTypstRenderer,
  type RenderSession,
  type TypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import * as rendererWrapper from "@myriaddreamin/typst-ts-renderer";
import { type BoundInteraction, type PageInteractions, type PageRect } from "../interactions";
import { CanvasStore } from "./canvas-store";
import { InteractionStore } from "./interaction-store";
import {
  clamp,
  clampPixelPerPt,
  computeInteractivePixelPerPt,
  isFullPageRender,
  pageDistanceToViewport,
  renderWindowKey,
  selectStagePagesAt,
  yieldToEventLoop,
} from "./render-utils";
import { hitRenderedTextBox, resolveRenderedTextRect } from "./text-hit";
import type {
  CanvasAck,
  CanvasLayer,
  PageLayout,
  PageRenderSpec,
  PageRenderResult,
  PageSpec,
  PendingCanvasAck,
  RenderQuality,
  WorkerConfig,
  WorkerPost,
} from "./types";

const visibleRenderScreens = 1;
const scrollRenderScreens = 1;
const prefetchRenderScreens = 5;
const idlePrefetchDelayMs = 120;
const maxCanvasBufferPages = 18;

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
  private readonly canvasStore = new CanvasStore(maxCanvasBufferPages);
  private readonly interactionStore = new InteractionStore();
  private readonly pagePixelPerPt = new Map<number, number>();
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
      if (this.isRenderInterrupted(frameVersion) || this.latestPages.length === 0) {
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
        await this.renderVisiblePages({
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
      this.canvasStore.setCanvas(page.index, page.canvas, page.widthPx, page.heightPx);
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

  async requestInteractions(request: { generation: number; pageIndices: number[] }) {
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

    const result = hitRenderedTextBox(
      this.canvasStore.get(request.pageIndex),
      this.pagePixelPerPt.get(request.pageIndex),
      request.x,
      request.y,
      request.rect,
    );
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

      const rect = resolveRenderedTextRect(
        this.canvasStore.get(request.pageIndex),
        this.pagePixelPerPt.get(request.pageIndex),
        request.rect,
      );
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

  resetDocumentState() {
    this.initializedDocument = false;
    this.generation = 0;
    this.documentVersion += 1;
    this.viewportVersion += 1;
    this.viewportRenderQueued = false;
    this.clearIdlePrefetchTimer();
    this.latestPages = [];
    this.canvasStore.clear();
    this.interactionStore.clear();
    this.pagePixelPerPt.clear();
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

      interactions.push(
        await this.interactionStore.renderPage(
          session,
          pageIndex,
          pixelPerPt,
          this.interactionCacheKey(pageIndex),
        ),
      );
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
      for (const index of this.canvasStore.indices()) {
        this.canvasStore.deleteRenderCacheKey(index);
      }
      this.interactionStore.clear();
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
    await this.renderVisiblePages({
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

  private async renderVisiblePages(options: {
    session: RenderSession;
    pages: PageSpec[];
    generation: number;
    kind: string;
    frameBytes: number;
    frameVersion: number;
    viewportVersion: number;
  }) {
    if (this.isRenderInterrupted(options.frameVersion)) {
      return;
    }

    await this.renderAndPostComplete({
      ...options,
      pages: this.selectStagePages(options.pages, visibleRenderScreens, { windowed: false }),
      phase: "visible",
      layer: "full",
      quality: "full",
      useCacheKey: true,
      updateCacheKey: true,
      collectInteractions: true,
      prioritizeViewport: true,
      cancelOnViewportChange: true,
      pauseWhenDragging: true,
      viewportVersion: options.viewportVersion,
    });
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
    for (const index of this.canvasStore.indices()) {
      if (!livePages.has(index)) {
        this.canvasStore.deletePage(index);
        this.pagePixelPerPt.delete(index);
        this.interactionStore.deletePage(index);
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

      const entry = this.canvasStore.get(page.index);
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
        this.canvasStore.touch(page.index);
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
        this.canvasStore.touch(page.index);
        pushResult({ pageIndex: page.index, quality: entry.quality });
        continue;
      }

      const widthPx = Math.max(1, Math.ceil(page.width * pixelPerPt));
      const heightPx = Math.max(1, Math.ceil(page.height * pixelPerPt));
      const resized = this.canvasStore.resizeEntry(entry, page.index, widthPx, heightPx);
      const context = this.canvasStore.ensureContext(entry, page.index);
      const fullPageRender = isFullPageRender(page);
      const fullPageResult = options.quality === "full" && fullPageRender;
      if (!fullPageRender && (resized || !entry.hasContent)) {
        this.canvasStore.initializeTargetCanvas(context, entry);
      }
      const cacheKey =
        options.useCacheKey && fullPageRender && entry.hasContent && entry.quality === "full"
          ? this.canvasStore.getRenderCacheKey(page.index)
          : undefined;
      const renderContext = fullPageRender
        ? context
        : this.canvasStore.ensureScratchContext(entry, page.index, options.layer);

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
          this.canvasStore.releaseScratch(entry);
        }
        break;
      }
      const stopAfterCommit = this.shouldStopRender(options);

      const cacheHit = !!cacheKey && result.cacheKey === cacheKey;
      if (!cacheHit && fullPageRender) {
        entry.hasContent = true;
      } else if (!cacheHit) {
        this.canvasStore.commitScratchToTarget(entry, context, renderContext, page, pixelPerPt);
        entry.hasContent = true;
      }
      entry.quality = options.quality;
      entry.renderedGeneration = options.generation;
      entry.renderedPixelPerPt = pixelPerPt;
      entry.renderedWindowKey = windowKey;
      if (!fullPageRender) {
        this.canvasStore.releaseScratch(entry);
      }
      if (entry.hasContent) {
        this.canvasStore.touch(page.index);
      }

      if (options.quality === "full" && result.cacheKey) {
        this.canvasStore.setLatestCacheKey(page.index, result.cacheKey);
      }
      if (
        options.quality === "full" &&
        options.updateCacheKey &&
        fullPageRender &&
        result.cacheKey
      ) {
        this.canvasStore.setRenderCacheKey(page.index, result.cacheKey);
      }

      if (stopAfterCommit) {
        pushResult({ pageIndex: page.index, quality: options.quality, fullPage: fullPageResult });
      } else if (options.quality === "full" && options.collectInteractions) {
        pushResult({
          pageIndex: page.index,
          quality: options.quality,
          fullPage: fullPageResult,
          interactions: await this.interactionStore.renderPage(
            session,
            page.index,
            pixelPerPt,
            this.interactionCacheKey(page.index),
          ),
        });
      } else if (options.quality === "full" && result.cacheKey) {
        const hadInteractions = this.interactionStore.invalidateIfCacheKeyChanged(
          page.index,
          result.cacheKey,
        );
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

  private enforceCanvasBuffer(generation: number) {
    const protectedPages = new Set(
      this.selectStagePages(this.latestPages, prefetchRenderScreens, { windowed: false }).map(
        (page) => page.index,
      ),
    );
    const evicted = this.canvasStore.enforceLimit(protectedPages);
    for (const pageIndex of evicted) {
      this.interactionStore.deletePage(pageIndex);
    }

    if (evicted.length > 0) {
      this.post({
        type: "render-evicted",
        generation,
        pageIndices: evicted,
      });
    }
  }

  private takeNextViewportPage(pages: PageRenderSpec[]) {
    if (pages.length === 0) {
      return undefined;
    }

    let bestIndex = 0;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (let index = 0; index < pages.length; index += 1) {
      const distance = pageDistanceToViewport(
        pages[index],
        this.latestPageLayouts,
        this.config.viewport || {},
      );
      if (distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }

    return pages.splice(bestIndex, 1)[0];
  }

  private interactionCacheKey(pageIndex: number) {
    return (
      this.canvasStore.getLatestCacheKey(pageIndex) || this.canvasStore.getRenderCacheKey(pageIndex)
    );
  }

  private selectStagePages(
    pages: PageSpec[],
    screens: number,
    options: { windowed?: boolean } = {},
  ): PageRenderSpec[] {
    const viewport = this.config.viewport || {};
    const viewportTop = Math.max(0, viewport.scrollTop || 0);
    return selectStagePagesAt(
      pages,
      this.latestPageLayouts,
      viewport,
      screens,
      viewportTop,
      options,
    );
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
