import type { PreviewElements } from "./dom";
import { installPageControls } from "./controls";
import {
  PageInteractionController,
  type BoundInteraction,
  type PageInteractions,
  type PageRect,
} from "./interactions";
import {
  applyPageLayout as applyLayout,
  collectPageLayouts as collectLayouts,
  currentViewportPage as currentPageFromLayouts,
  nextZoomRatio,
  readPageLayoutMetrics as readLayoutMetrics,
  readViewportSnapshot as buildViewportSnapshot,
  restoreZoomAnchor,
  viewportPostKey,
  zoomAnchorFromEvent,
  type PageLayoutMetrics,
  type PageLayoutRecord,
} from "./layout";
import {
  parseCursorPosition,
  renderCursor,
  scrollViewportToTypstLocation,
  showJumpMarker,
} from "./navigation";
import { renderOutline } from "./outline";
import { createPageRecord } from "./page-record";
import { parseInvertColorStrategy, parsePreviewPositions } from "./protocol";
import { InvertColorController } from "./theme";
import type {
  PageRecord,
  PageSpec,
  PreviewMode,
  PreviewPosition,
  ViewportSnapshot,
} from "./types";
import { clamp } from "./utils";

interface PageHostOptions {
  elements: PreviewElements;
  postWorker: (message: unknown, transfer?: Transferable[]) => void;
}

/** Owns preview page DOM, render-worker page requests, and routed preview protocol messages. */
export class PageHost {
  private readonly elements: PreviewElements;
  private readonly postWorker: PageHostOptions["postWorker"];
  private previewMode: PreviewMode = "Doc";
  private contentPreview = false;
  private currentSlidePage = 1;
  private pageCount = 0;
  private zoomRatio = 1;
  private dragging = false;
  private scrolling = false;
  private lastViewportPostKey = "";
  private lastViewportTimer = 0;
  private scrollIdleTimer = 0;
  private sourceSyncEchoIgnoreUntil = 0;
  private sourceSyncEchoIgnoreCount = 0;
  private lastPages: PageSpec[] = [];
  private pendingCursor: PreviewPosition | undefined;
  private outlineData: any;
  private readonly invertColors = new InvertColorController();
  private readonly interactions: PageInteractionController;
  private readonly pageRecords = new Map<number, PageRecord>();

  constructor({ elements, postWorker }: PageHostOptions) {
    this.elements = elements;
    this.postWorker = postWorker;
    this.interactions = new PageInteractionController({
      postWorker,
      getRecord: (pageIndex) => this.pageRecords.get(pageIndex),
      getRecords: () => this.pageRecords.values(),
      getViewport: () => this.elements.viewport,
      isDragging: () => this.dragging,
      isContentPreview: () => this.contentPreview,
      scrollToTypstLocation: (position) => this.scrollToTypstLocation(position),
      onSourceSyncRequest: () => this.markSourceSyncEchoSuppression(),
    });
  }

  get mode() {
    return this.previewMode;
  }

  get slidePage() {
    return this.currentSlidePage;
  }

  installControls() {
    installPageControls(this.elements, this);
  }

  goToPreviousSlide() {
    this.setCurrentSlidePage(this.currentSlidePage - 1);
  }

  goToNextSlide() {
    this.setCurrentSlidePage(this.currentSlidePage + 1);
  }

  toggleInvertColors() {
    this.elements.root.classList.toggle("invert-colors");
  }

  setDragging(active: boolean) {
    if (this.dragging === active) {
      return;
    }
    this.dragging = active;
    this.elements.root.classList.toggle("dragging", active);
    this.interactions.handleDraggingChanged(active);
    if (!active) {
      this.scrolling = false;
      if (this.scrollIdleTimer) {
        window.clearTimeout(this.scrollIdleTimer);
        this.scrollIdleTimer = 0;
      }
    }
    this.postViewportSnapshot({
      requestInteractions: !active,
      renderDuringDrag: !active,
      force: true,
    });
  }

  applyWheelZoom(event: WheelEvent) {
    const previousZoom = this.zoomRatio;
    this.zoomRatio = nextZoomRatio(this.zoomRatio, event.deltaY);
    if (this.zoomRatio === previousZoom) {
      return;
    }

    const metrics = this.readPageLayoutMetrics();
    const anchor = zoomAnchorFromEvent(
      event,
      this.elements.viewport,
      this.pageRecords,
      metrics,
      this.collectPageLayouts(),
      this.contentPreview,
    );
    this.applyAllPageLayouts(metrics);
    restoreZoomAnchor(
      anchor,
      this.elements.viewport,
      this.pageRecords,
      metrics,
      this.collectPageLayouts(),
      this.contentPreview,
    );
    this.scheduleViewportSnapshot();
  }

  setPreviewMode(mode: PreviewMode) {
    this.previewMode = mode;
    this.elements.root.classList.toggle("mode-slide", mode === "Slide");
    this.elements.root.classList.toggle("mode-doc", mode === "Doc");
    this.applyAllPageLayouts(this.readPageLayoutMetrics());
  }

  setContentPreview(enabled: boolean) {
    this.contentPreview = enabled;
    this.elements.root.classList.toggle("content-preview", enabled);
    this.renderOutline();
  }

  setOutlineData(outline: any) {
    this.outlineData = outline;
    this.renderOutline();
  }

  ensurePages(generation: number, pages: PageSpec[]) {
    if (!("transferControlToOffscreen" in HTMLCanvasElement.prototype)) {
      throw new Error("OffscreenCanvas transfer is not supported in this browser");
    }

    this.lastPages = pages;
    this.updatePageCount(pages.length);
    if (this.interactions.startGeneration(generation)) {
      for (const record of this.pageRecords.values()) {
        record.container.classList.remove("full-ready");
      }
    }

    const livePages = new Set<number>();
    const metrics = this.readPageLayoutMetrics();
    const transferred: Array<{
      index: number;
      layer: "preview" | "full";
      canvas: OffscreenCanvas;
      widthPx: number;
      heightPx: number;
    }> = [];

    for (const page of pages) {
      livePages.add(page.index);
      const widthPx = Math.max(1, Math.ceil(page.width * page.pixelPerPt));
      const heightPx = Math.max(1, Math.ceil(page.height * page.pixelPerPt));
      const key = [page.width.toFixed(3), page.height.toFixed(3), page.pixelPerPt].join(":");
      let record = this.pageRecords.get(page.index);

      if (!record || record.key !== key) {
        record?.container.remove();
        record = createPageRecord(page, key, widthPx, heightPx);
        this.interactions.installPageHandlers(record);
        this.pageRecords.set(page.index, record);
        this.insertPage(record, page.index);
      }

      applyLayout(record, page, metrics, {
        previewMode: this.previewMode,
        currentSlidePage: this.currentSlidePage,
        zoomRatio: this.zoomRatio,
      });
      if (!record.transferred) {
        record.canvas.width = 1;
        record.canvas.height = 1;
        record.fullWidthPx = widthPx;
        record.fullHeightPx = heightPx;
        const offscreen = record.canvas.transferControlToOffscreen();
        record.transferred = true;
        transferred.push({
          index: page.index,
          layer: "full",
          canvas: offscreen,
          widthPx,
          heightPx,
        });
      }
    }

    for (const [index, record] of this.pageRecords) {
      if (!livePages.has(index)) {
        this.interactions.removePage(index);
        record.container.remove();
        this.pageRecords.delete(index);
      }
    }

    this.renderCursor();
    this.renderOutline();
    const ack = { layouts: this.collectPageLayouts() };
    this.postWorker(
      { type: "canvases", generation, canvases: transferred, ack },
      transferred.map((page) => page.canvas),
    );
  }

  collectPageLayouts(): PageLayoutRecord[] {
    return collectLayouts(this.lastPages, this.pageRecords, this.pageGap());
  }

  readViewportSnapshot(metrics = this.readPageLayoutMetrics()): ViewportSnapshot {
    return buildViewportSnapshot(this.elements.viewport, metrics, {
      dragging: this.dragging,
      scrolling: this.scrolling,
    });
  }

  scheduleViewportSnapshot(options: { applyLayouts?: boolean; scrolling?: boolean } = {}) {
    if (options.scrolling) {
      this.scrolling = true;
      if (this.scrollIdleTimer) {
        window.clearTimeout(this.scrollIdleTimer);
      }
      this.scrollIdleTimer = window.setTimeout(() => {
        this.scrollIdleTimer = 0;
        this.scrolling = false;
        this.postViewportSnapshot({ requestInteractions: !this.dragging, force: true });
      }, 120);
      if (this.lastViewportTimer) {
        clearTimeout(this.lastViewportTimer);
        this.lastViewportTimer = 0;
      }
      this.postViewportSnapshot({
        applyLayouts: options.applyLayouts,
        requestInteractions: false,
      });
      return;
    }
    if (this.lastViewportTimer) {
      clearTimeout(this.lastViewportTimer);
    }
    this.lastViewportTimer = window.setTimeout(
      () => {
        this.lastViewportTimer = 0;
        this.postViewportSnapshot({
          applyLayouts: options.applyLayouts,
          requestInteractions: !this.dragging,
        });
      },
      options.scrolling ? 0 : 16,
    );
  }

  private postViewportSnapshot(
    options: {
      applyLayouts?: boolean;
      force?: boolean;
      requestInteractions?: boolean;
      renderDuringDrag?: boolean;
    } = {},
  ) {
    const metrics = this.readPageLayoutMetrics();
    if (options.applyLayouts) {
      this.applyAllPageLayouts(metrics);
    }
    const layouts = this.collectPageLayouts();
    const viewport = this.readViewportSnapshot(metrics);
    if (options.renderDuringDrag !== undefined) {
      viewport.renderDuringDrag = options.renderDuringDrag;
    }
    const key = viewportPostKey(viewport);
    if (!options.force && key === this.lastViewportPostKey) {
      return;
    }
    this.lastViewportPostKey = key;
    this.postWorker({
      type: "viewport",
      viewport,
      layouts,
    });
    if (options.requestInteractions ?? !this.dragging) {
      this.interactions.requestViewportInteractions(layouts);
    }
  }

  handlePreviewProtocolMessage(kind: string, text: string) {
    switch (kind) {
      case "jump":
      case "viewport":
        if (this.shouldIgnoreSourceSyncEcho()) {
          return;
        }
        this.handleJumpMessage(text);
        break;
      case "cursor":
        this.handleCursorMessage(text);
        break;
      case "cursor-paths":
        break;
      case "partial-rendering":
        break;
      case "invert-colors":
        this.invertColors.apply(this.elements.root, parseInvertColorStrategy(text));
        break;
      case "outline":
        try {
          this.setOutlineData(JSON.parse(text));
        } catch (_error) {}
        break;
    }
  }

  private markSourceSyncEchoSuppression() {
    this.sourceSyncEchoIgnoreUntil = performance.now() + 1_200;
    this.sourceSyncEchoIgnoreCount = 2;
  }

  private shouldIgnoreSourceSyncEcho() {
    if (this.sourceSyncEchoIgnoreCount <= 0) {
      return false;
    }
    if (performance.now() > this.sourceSyncEchoIgnoreUntil) {
      this.sourceSyncEchoIgnoreCount = 0;
      return false;
    }
    this.sourceSyncEchoIgnoreCount -= 1;
    return true;
  }

  updateInteractions(
    generation: number,
    interactions: PageInteractions[],
    invalidatedPageIndices: number[] = [],
  ) {
    this.interactions.updateInteractions(generation, interactions, invalidatedPageIndices);
  }

  markRendered(
    generation: number,
    layer: "full" | undefined,
    quality: "preview" | "full" | undefined,
    pageIndices: number[],
    fullPageIndices: number[] = [],
  ) {
    if (!this.interactions.matchesGeneration(generation)) {
      return;
    }
    if (layer !== "full") {
      return;
    }

    for (const pageIndex of pageIndices) {
      const record = this.pageRecords.get(pageIndex);
      if (!record) {
        continue;
      }
      record.container.classList.add("canvas-ready");
    }

    if (quality !== "full") {
      return;
    }

    for (const pageIndex of fullPageIndices) {
      const record = this.pageRecords.get(pageIndex);
      if (record) {
        record.container.classList.add("full-ready");
      }
    }
  }

  markEvicted(generation: number, pageIndices: number[]) {
    if (!this.interactions.matchesGeneration(generation)) {
      return;
    }
    for (const pageIndex of pageIndices) {
      const record = this.pageRecords.get(pageIndex);
      if (!record) {
        continue;
      }
      record.container.classList.remove("canvas-ready");
      record.container.classList.remove("full-ready");
    }
    this.interactions.markEvicted(generation, pageIndices);
  }

  handleBoundHit(message: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    bound?: BoundInteraction;
  }) {
    this.interactions.handleBoundHit(message);
  }

  handleTextHit(message: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    hit: boolean;
    rect?: PageRect;
  }) {
    this.interactions.handleTextHit(message);
  }

  handleTextRect(message: {
    generation: number;
    pageIndex: number;
    textId: number;
    rect?: PageRect;
  }) {
    this.interactions.handleTextRect(message);
  }

  clearPages() {
    for (const record of this.pageRecords.values()) {
      record.container.remove();
    }
    this.pageRecords.clear();
    this.lastPages = [];
    this.lastViewportPostKey = "";
    this.pendingCursor = undefined;
    this.sourceSyncEchoIgnoreUntil = 0;
    this.sourceSyncEchoIgnoreCount = 0;
    this.interactions.resetAll();
    this.updatePageCount(0);
  }

  private insertPage(record: PageRecord, index: number) {
    if (index === 0) {
      this.elements.outlinePanel.after(record.container);
      return;
    }

    const previous = this.pageRecords.get(index - 1);
    if (previous?.container.parentElement) {
      previous.container.after(record.container);
      return;
    }

    this.elements.pages.append(record.container);
  }

  private readPageLayoutMetrics(): PageLayoutMetrics {
    return readLayoutMetrics(this.elements.viewport);
  }

  private pageGap() {
    return this.contentPreview ? 5 : 10;
  }

  private handleJumpMessage(text: string) {
    const positions = parsePreviewPositions(text);
    if (positions.length === 0) {
      return;
    }

    const currentPage =
      this.previewMode === "Slide"
        ? this.currentSlidePage
        : currentPageFromLayouts(this.elements.viewport, this.collectPageLayouts());
    const target = positions.reduce((best, position) =>
      Math.abs(position.page - currentPage) <= Math.abs(best.page - currentPage) ? position : best,
    );
    if (this.previewMode === "Slide") {
      this.setCurrentSlidePage(target.page);
      return;
    }

    this.scrollToTypstLocation(target);
  }

  private handleCursorMessage(text: string) {
    const position = parseCursorPosition(text);
    if (!position) {
      return;
    }

    this.pendingCursor = position;
    if (this.previewMode === "Slide") {
      this.setCurrentSlidePage(position.page);
    }
    this.renderCursor();
  }

  private scrollToTypstLocation(position: PreviewPosition) {
    const record = this.pageRecords.get(position.page - 1);
    if (!record) {
      return;
    }

    const { xRatio, yRatio } = scrollViewportToTypstLocation(
      this.elements.viewport,
      record,
      position,
    );
    showJumpMarker(record, xRatio, yRatio);
    this.scheduleViewportSnapshot();
  }

  private renderCursor() {
    renderCursor(this.pageRecords.values(), this.pendingCursor);
  }

  private setCurrentSlidePage(page: number) {
    if (!Number.isFinite(page) || this.pageCount === 0) {
      return;
    }
    this.currentSlidePage = clamp(Math.trunc(page), 1, this.pageCount);
    this.applyAllPageLayouts(this.readPageLayoutMetrics());
    this.renderCursor();
    this.elements.viewport.scrollTo({ top: 0, left: 0, behavior: "auto" });
    this.scheduleViewportSnapshot();
  }

  private updatePageCount(nextPageCount: number) {
    this.pageCount = nextPageCount;
    this.currentSlidePage = clamp(this.currentSlidePage, 1, Math.max(1, this.pageCount));
  }

  private applyAllPageLayouts(metrics = this.readPageLayoutMetrics()) {
    for (const page of this.lastPages) {
      const record = this.pageRecords.get(page.index);
      if (record) {
        applyLayout(record, page, metrics, {
          previewMode: this.previewMode,
          currentSlidePage: this.currentSlidePage,
          zoomRatio: this.zoomRatio,
        });
      }
    }
    this.renderCursor();
  }

  private renderOutline() {
    renderOutline(this.elements.outlinePanel, this.outlineData, this.contentPreview, (position) =>
      this.scrollToTypstLocation(position),
    );
  }
}
