import type { PreviewElements } from "./dom";
import { installPageControls } from "./controls";
import {
  hitTestLink,
  hitTestText,
  textHighlightsForLink,
  type BoundInteraction,
  type LinkInteraction,
  type PageInteractions,
  type PageRect,
  type TextInteraction,
} from "./interactions";
import { parseInvertColorStrategy, parsePreviewPositions } from "./protocol";
import type {
  InvertColorStrategy,
  InvertColorStrategyMap,
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

interface PageLayoutMetrics {
  availableWidth: number;
  availableHeight: number;
}

interface PageLayoutRecord {
  index: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
  scale: number;
  scaleX: number;
  scaleY: number;
}

interface ZoomAnchor {
  pageIndex: number;
  pageX: number;
  pageY: number;
  viewportX: number;
  viewportY: number;
}

/** Owns preview page DOM and controls, handling render-worker page requests and routed preview protocol messages. */
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
  private lastPages: PageSpec[] = [];
  private pendingCursor: PreviewPosition | undefined;
  private outlineData: any;
  private autoInvertDecision: boolean | undefined;
  private interactionGeneration = 0;
  private pointerDown:
    | {
        pageIndex: number;
        x: number;
        y: number;
      }
    | undefined;
  private lastPointer:
    | {
        pageIndex: number;
        x: number;
        y: number;
      }
    | undefined;
  private activeHoverKey = "";
  private activeTextHover:
    | {
        generation: number;
        pageIndex: number;
        rect: PageRect;
      }
    | undefined;
  private readonly textHoverRects = new Map<string, PageRect>();
  private readonly pendingTextRectRequests = new Set<string>();
  private readonly pendingInteractionRequests = new Set<number>();
  private nextHitRequestId = 0;
  private pendingTextHover:
    | {
        requestId: number;
        generation: number;
        pageIndex: number;
        x: number;
        y: number;
        text: TextInteraction;
      }
    | undefined;
  private pendingBoundHover:
    | {
        requestId: number;
        generation: number;
        pageIndex: number;
        x: number;
        y: number;
      }
    | undefined;
  private readonly pageRecords = new Map<number, PageRecord>();

  constructor({ elements, postWorker }: PageHostOptions) {
    this.elements = elements;
    this.postWorker = postWorker;
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
    if (active) {
      this.activeHoverKey = "";
      this.activeTextHover = undefined;
      this.pendingTextHover = undefined;
      this.pendingBoundHover = undefined;
    } else {
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
    if (event.deltaY < 0) {
      this.zoomRatio = zoomFactors.find((factor) => factor > this.zoomRatio) ?? this.zoomRatio;
    } else if (event.deltaY > 0) {
      this.zoomRatio =
        [...zoomFactors].reverse().find((factor) => factor < this.zoomRatio) ?? this.zoomRatio;
    }
    if (this.zoomRatio === previousZoom) {
      return;
    }

    const metrics = this.readPageLayoutMetrics();
    const anchor = this.zoomAnchorFromEvent(event, metrics, this.collectPageLayouts());
    this.applyAllPageLayouts(metrics);
    this.restoreZoomAnchor(anchor, metrics, this.collectPageLayouts());
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
    if (this.interactionGeneration !== generation) {
      this.interactionGeneration = generation;
      this.textHoverRects.clear();
      this.pendingTextRectRequests.clear();
      this.pendingInteractionRequests.clear();
      this.activeTextHover = undefined;
      this.pendingTextHover = undefined;
      this.pendingBoundHover = undefined;
      for (const record of this.pageRecords.values()) {
        record.interactions = undefined;
        this.renderLinkAnchors(record);
        record.container.classList.remove("canvas-ready");
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
        record = this.createPageRecord(page, key, widthPx, heightPx);
        this.pageRecords.set(page.index, record);
        this.insertPage(record, page.index);
      }

      this.applyPageLayout(record, page, metrics);
      if (!record.transferred) {
        record.canvas.width = 1;
        record.canvas.height = 1;
        record.fullWidthPx = widthPx;
        record.fullHeightPx = heightPx;
        const offscreen = record.canvas.transferControlToOffscreen();
        record.transferred = true;
        transferred.push({ index: page.index, layer: "full", canvas: offscreen, widthPx, heightPx });
      }
    }

    for (const [index, record] of this.pageRecords) {
      if (!livePages.has(index)) {
        if (this.lastPointer?.pageIndex === index) {
          this.lastPointer = undefined;
          this.activeHoverKey = "";
        }
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
    const layouts: PageLayoutRecord[] = [];
    let top = 0;
    let visibleCount = 0;
    const gap = this.pageGap();

    for (const page of this.lastPages) {
      const record = this.pageRecords.get(page.index);
      if (!record) {
        continue;
      }

      if (!record.container.hidden) {
        if (visibleCount > 0) {
          top += gap;
        }
        const width = record.cssWidth;
        const height = record.cssHeight;
        const scaleX = width / Math.max(record.width, 1);
        const scaleY = height / Math.max(record.height, 1);
        layouts.push({
          index: record.index,
          top,
          bottom: top + height,
          width,
          height,
          scale: scaleY,
          scaleX,
          scaleY,
        });
        top += height;
        visibleCount += 1;
        continue;
      }
    }

    return layouts;
  }

  readViewportSnapshot(metrics = this.readPageLayoutMetrics()): ViewportSnapshot {
    const viewport = this.elements.viewport;
    const width = metrics.availableWidth;
    const height = metrics.availableHeight;
    return {
      width,
      height,
      scrollLeft: viewport.scrollLeft,
      scrollTop: viewport.scrollTop,
      devicePixelRatio: window.devicePixelRatio || 1,
      dragging: this.dragging,
      scrolling: this.scrolling,
      window: {
        innerWidth: window.innerWidth,
        innerHeight: window.innerHeight,
      },
      boundingRect: {
        x: 0,
        y: 0,
        width,
        height,
        top: 0,
        right: width,
        bottom: height,
        left: 0,
      },
    };
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
    this.lastViewportTimer = window.setTimeout(() => {
      this.lastViewportTimer = 0;
      this.postViewportSnapshot({
        applyLayouts: options.applyLayouts,
        requestInteractions: !this.dragging,
      });
    }, options.scrolling ? 0 : 16);
  }

  private postViewportSnapshot(options: {
    applyLayouts?: boolean;
    force?: boolean;
    requestInteractions?: boolean;
    renderDuringDrag?: boolean;
  } = {}) {
    const metrics = this.readPageLayoutMetrics();
    if (options.applyLayouts) {
      this.applyAllPageLayouts(metrics);
    }
    const layouts = this.collectPageLayouts();
    const viewport = this.readViewportSnapshot(metrics);
    if (options.renderDuringDrag !== undefined) {
      viewport.renderDuringDrag = options.renderDuringDrag;
    }
    const key = this.viewportPostKey(viewport);
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
      this.requestViewportInteractions(layouts);
    }
  }

  private viewportPostKey(viewport: ViewportSnapshot) {
    return [
      viewport.width,
      viewport.height,
      viewport.scrollLeft,
      viewport.scrollTop,
      viewport.devicePixelRatio,
      viewport.dragging ? 1 : 0,
      viewport.scrolling ? 1 : 0,
      viewport.renderDuringDrag === false ? 0 : 1,
    ].join(":");
  }

  handlePreviewProtocolMessage(kind: string, text: string) {
    switch (kind) {
      case "jump":
      case "viewport":
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
        this.ensureInvertColors(parseInvertColorStrategy(text));
        break;
      case "outline":
        try {
          this.outlineData = JSON.parse(text);
          this.renderOutline();
        } catch (_error) {}
        break;
    }
  }

  updateInteractions(
    generation: number,
    interactions: PageInteractions[],
    invalidatedPageIndices: number[] = [],
  ) {
    if (generation !== this.interactionGeneration) {
      return;
    }
    let refreshHover = false;
    for (const pageIndex of invalidatedPageIndices) {
      const record = this.pageRecords.get(pageIndex);
      if (record) {
        record.interactions = undefined;
        this.renderLinkAnchors(record);
        refreshHover ||= this.lastPointer?.pageIndex === record.index;
      }
      this.pendingInteractionRequests.delete(pageIndex);
    }
    for (const pageInteractions of interactions) {
      const record = this.pageRecords.get(pageInteractions.pageIndex);
      if (record) {
        record.interactions = pageInteractions;
        this.renderLinkAnchors(record);
        refreshHover ||= this.lastPointer?.pageIndex === record.index;
      }
      this.pendingInteractionRequests.delete(pageInteractions.pageIndex);
    }
    if (refreshHover) {
      this.activeHoverKey = "";
      this.restoreInteractionHover();
    }
  }

  markRendered(
    generation: number,
    layer: "full" | undefined,
    quality: "preview" | "full" | undefined,
    pageIndices: number[],
    fullPageIndices: number[] = [],
  ) {
    if (generation !== this.interactionGeneration) {
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
    if (generation !== this.interactionGeneration) {
      return;
    }
    let refreshHover = false;
    for (const pageIndex of pageIndices) {
      const record = this.pageRecords.get(pageIndex);
      if (!record) {
        continue;
      }
      record.container.classList.remove("canvas-ready");
      record.container.classList.remove("full-ready");
      record.interactions = undefined;
      this.renderLinkAnchors(record);
      this.pendingInteractionRequests.delete(pageIndex);
      refreshHover ||= this.lastPointer?.pageIndex === pageIndex;
    }
    if (refreshHover) {
      this.activeHoverKey = "";
      this.activeTextHover = undefined;
      this.restoreInteractionHover();
    }
  }

  handleBoundHit(message: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    bound?: BoundInteraction;
  }) {
    const pending = this.pendingBoundHover;
    if (
      !pending ||
      pending.requestId !== message.requestId ||
      pending.generation !== message.generation ||
      pending.pageIndex !== message.pageIndex ||
      this.interactionGeneration !== message.generation
    ) {
      return;
    }

    if (
      !this.lastPointer ||
      this.lastPointer.pageIndex !== message.pageIndex ||
      Math.hypot(this.lastPointer.x - message.x, this.lastPointer.y - message.y) > 0.5
    ) {
      return;
    }

    const record = this.pageRecords.get(message.pageIndex);
    if (!record) {
      return;
    }

    if (!message.bound) {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      return;
    }

    this.showBoundHover(record, message.bound);
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
    const pending = this.pendingTextHover;
    if (
      !pending ||
      pending.requestId !== message.requestId ||
      pending.generation !== message.generation ||
      pending.pageIndex !== message.pageIndex ||
      this.interactionGeneration !== message.generation
    ) {
      return;
    }

    if (
      !this.lastPointer ||
      this.lastPointer.pageIndex !== message.pageIndex ||
      Math.hypot(this.lastPointer.x - message.x, this.lastPointer.y - message.y) > 0.5
    ) {
      return;
    }

    const record = this.pageRecords.get(message.pageIndex);
    if (!record) {
      return;
    }

    if (
      message.hit &&
      this.textHitContains(pending.text, message.rect, message.x, message.y)
    ) {
      this.showTextHover(record, pending.text, message.rect);
      return;
    }

    this.clearInteractionHighlight(record);
    this.activeHoverKey = "";
    this.requestBoundHover(record, { x: message.x, y: message.y });
  }

  handleTextRect(message: {
    generation: number;
    pageIndex: number;
    textId: number;
    rect?: PageRect;
  }) {
    if (message.generation !== this.interactionGeneration) {
      return;
    }

    const key = this.textHoverKey(message.pageIndex, message.textId);
    this.pendingTextRectRequests.delete(key);
    if (message.rect) {
      this.textHoverRects.set(key, message.rect);
    }

    if (this.lastPointer?.pageIndex === message.pageIndex) {
      this.activeHoverKey = "";
      this.restoreInteractionHover();
    }
  }

  clearPages() {
    for (const record of this.pageRecords.values()) {
      record.container.remove();
    }
    this.pageRecords.clear();
    this.lastPages = [];
    this.lastViewportPostKey = "";
    this.pendingCursor = undefined;
    this.interactionGeneration = 0;
    this.pointerDown = undefined;
    this.lastPointer = undefined;
    this.activeHoverKey = "";
    this.activeTextHover = undefined;
    this.textHoverRects.clear();
    this.pendingTextRectRequests.clear();
    this.pendingInteractionRequests.clear();
    this.pendingTextHover = undefined;
    this.pendingBoundHover = undefined;
    this.updatePageCount(0);
  }

  private createPageRecord(
    page: PageSpec,
    key: string,
    widthPx: number,
    heightPx: number,
  ): PageRecord {
    const container = document.createElement("div");
    container.className = "typst-page canvas-mode";
    container.dataset.pageNumber = String(page.index);

    const shell = document.createElement("div");
    shell.className = "typst-page-canvas";
    shell.dataset.pageNumber = String(page.index);
    shell.dataset.pageWidth = page.width.toFixed(3);
    shell.dataset.pageHeight = page.height.toFixed(3);

    const canvas = document.createElement("canvas");
    canvas.className = "typst-full-canvas";
    canvas.width = 1;
    canvas.height = 1;

    const interactionLayer = document.createElement("div");
    interactionLayer.className = "typst-interaction-layer";

    const linkLayer = document.createElement("div");
    linkLayer.className = "typst-link-layer";

    const cursor = document.createElement("div");
    cursor.className = "typst-cursor";

    const jumpMarker = document.createElement("div");
    jumpMarker.className = "typst-jump-marker";

    shell.append(canvas);
    container.append(shell, linkLayer, interactionLayer, cursor, jumpMarker);

    const record = {
      index: page.index,
      key,
      container,
      shell,
      canvas,
      linkLayer,
      interactionLayer,
      cursor,
      jumpMarker,
      transferred: false,
      width: page.width,
      height: page.height,
      fullWidthPx: widthPx,
      fullHeightPx: heightPx,
      cssWidth: widthPx,
      cssHeight: heightPx,
      pixelPerPt: page.pixelPerPt,
    };
    this.installPageInteractionHandlers(record);
    return record;
  }

  private installPageInteractionHandlers(record: PageRecord) {
    record.container.addEventListener("mousedown", (event) => {
      if (event.button !== 0) {
        return;
      }
      this.pointerDown = {
        pageIndex: record.index,
        x: event.clientX,
        y: event.clientY,
      };
    });

    record.container.addEventListener("mousemove", (event) => {
      this.handlePagePointerMove(record, event);
    });

    record.container.addEventListener("mouseleave", () => {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      if (this.lastPointer?.pageIndex === record.index) {
        this.lastPointer = undefined;
      }
    });

    record.container.addEventListener("click", (event) => {
      this.handlePageClick(record, event);
    });
  }

  private handlePagePointerMove(record: PageRecord, event: MouseEvent) {
    if (this.isDragging()) {
      if (this.activeHoverKey || record.interactionLayer.childElementCount > 0) {
        this.clearInteractionHighlight(record);
        this.activeHoverKey = "";
      }
      this.lastPointer = undefined;
      return;
    }

    const point = this.pagePointFromEvent(record, event);
    if (!point) {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      this.lastPointer = undefined;
      return;
    }

    this.lastPointer = { pageIndex: record.index, x: point.x, y: point.y };
    this.updateInteractionHover(record, point);
  }

  private updateInteractionHover(record: PageRecord, point: { x: number; y: number }) {
    if (!record.interactions) {
      this.requestPageInteractions([record.index]);
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      return;
    }

    const link = hitTestLink(record.interactions, point.x, point.y);
    if (link) {
      this.showLinkHover(record, link);
      return;
    }

    if (this.activeTextHoverContains(record, point)) {
      record.container.style.cursor = "text";
      return;
    }

    const cachedText = this.cachedTextHoverAt(record, point);
    if (cachedText) {
      this.showTextHoverRect(record, cachedText.text, cachedText.rect);
      return;
    }

    const text = hitTestText(record.interactions, point.x, point.y);
    if (text) {
      this.requestTextHover(record, text, point);
      return;
    }

    this.requestBoundHover(record, point);
  }

  private restoreInteractionHover() {
    if (!this.lastPointer) {
      return;
    }
    const record = this.pageRecords.get(this.lastPointer.pageIndex);
    if (!record) {
      return;
    }
    this.updateInteractionHover(record, this.lastPointer);
  }

  private requestViewportInteractions(layouts: PageLayoutRecord[]) {
    if (this.interactionGeneration <= 0) {
      return;
    }
    if (this.isDragging()) {
      return;
    }

    const viewportTop = this.elements.viewport.scrollTop;
    const viewportBottom = viewportTop + this.elements.viewport.clientHeight;
    const pageIndices = layouts
      .filter((layout) => layout.bottom >= viewportTop && layout.top <= viewportBottom)
      .map((layout) => layout.index);
    this.requestPageInteractions(pageIndices);
  }

  private requestPageInteractions(pageIndices: number[]) {
    const missing: number[] = [];
    const seen = new Set<number>();
    for (const pageIndex of pageIndices) {
      if (seen.has(pageIndex) || this.pendingInteractionRequests.has(pageIndex)) {
        continue;
      }
      seen.add(pageIndex);

      const record = this.pageRecords.get(pageIndex);
      if (!record || record.interactions) {
        continue;
      }

      this.pendingInteractionRequests.add(pageIndex);
      missing.push(pageIndex);
    }

    if (missing.length === 0) {
      return;
    }

    this.postWorker({
      type: "request-interactions",
      generation: this.interactionGeneration,
      pageIndices: missing,
    });
  }

  private isDragging() {
    return this.dragging;
  }

  private handlePageClick(record: PageRecord, event: MouseEvent) {
    if (this.isDragClick(record, event)) {
      return;
    }

    const point = this.pagePointFromEvent(record, event);
    if (!point) {
      return;
    }
    this.lastPointer = { pageIndex: record.index, x: point.x, y: point.y };

    if (!record.interactions) {
      this.requestPageInteractions([record.index]);
    }

    const link = hitTestLink(record.interactions, point.x, point.y);
    if (link?.target.kind === "internal" && link.target.position) {
      event.preventDefault();
      this.scrollToTypstLocation(link.target.position);
      return;
    }

    if (this.contentPreview) {
      this.postWorker({ type: "send", text: `outline-sync,${record.index + 1}` });
      return;
    }

    this.postWorker({
      type: "send",
      text: `src-point ${JSON.stringify({
        page_no: record.index + 1,
        x: point.x,
        y: point.y,
      })}`,
    });
  }

  private pagePointFromEvent(
    record: PageRecord,
    event: MouseEvent,
  ): { x: number; y: number } | undefined {
    const rect = record.shell.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      return undefined;
    }

    return {
      x: clamp(((event.clientX - rect.left) / rect.width) * record.width, 0, record.width),
      y: clamp(((event.clientY - rect.top) / rect.height) * record.height, 0, record.height),
    };
  }

  private isDragClick(record: PageRecord, event: MouseEvent) {
    const start = this.pointerDown;
    this.pointerDown = undefined;
    if (!start || start.pageIndex !== record.index) {
      return false;
    }

    const distance = Math.hypot(event.clientX - start.x, event.clientY - start.y);
    return distance > 4;
  }

  private showLinkHover(record: PageRecord, link: LinkInteraction) {
    const key = `link:${record.index}:${link.target.kind}:${link.target.href}:${link.rect.x}:${link.rect.y}`;
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = undefined;
    record.container.style.cursor = "pointer";

    const textRects = textHighlightsForLink(record.interactions, link).map((highlight) => {
      this.requestTextVisualRect(record, highlight.text);
      const visualRect = this.textHoverRects.get(this.textHoverKey(record.index, highlight.text.id));
      return visualRect ? alignTextClipToVisualRect(highlight.rect, visualRect) : highlight.rect;
    });
    this.renderInteractionHighlights(record, textRects, "link");
  }

  private showTextHover(record: PageRecord, text: TextInteraction, hitRect?: PageRect) {
    const visualRect = textHoverRect(text, hitRect);
    this.textHoverRects.set(this.textHoverKey(record.index, text.id), visualRect);
    this.showTextHoverRect(record, text, visualRect);
  }

  private showTextHoverRect(record: PageRecord, text: TextInteraction, visualRect: PageRect) {
    const key = `text:${record.index}:${text.id}:${visualRect.x}:${visualRect.y}:${visualRect.width}:${visualRect.height}`;
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = {
      generation: this.interactionGeneration,
      pageIndex: record.index,
      rect: visualRect,
    };
    record.container.style.cursor = "text";
    this.renderInteractionHighlights(record, [visualRect], "text");
  }

  private requestTextHover(
    record: PageRecord,
    text: TextInteraction,
    point: { x: number; y: number },
  ) {
    const key = `text-query:${record.index}:${text.id}:${point.x.toFixed(1)}:${point.y.toFixed(1)}`;
    if (this.activeHoverKey === key) {
      return;
    }

    const request = {
      requestId: ++this.nextHitRequestId,
      generation: this.interactionGeneration,
      pageIndex: record.index,
      x: point.x,
      y: point.y,
      text,
    };
    this.activeHoverKey = key;
    this.pendingTextHover = request;
    record.container.style.cursor = "";
    this.postWorker({
      type: "hit-text",
      requestId: request.requestId,
      generation: request.generation,
      pageIndex: request.pageIndex,
      x: request.x,
      y: request.y,
      rect: text.rect,
    });
  }

  private requestTextVisualRect(record: PageRecord, text: TextInteraction) {
    const key = this.textHoverKey(record.index, text.id);
    if (this.textHoverRects.has(key) || this.pendingTextRectRequests.has(key)) {
      return;
    }
    this.pendingTextRectRequests.add(key);
    this.postWorker({
      type: "resolve-text-rect",
      requestId: ++this.nextHitRequestId,
      generation: this.interactionGeneration,
      pageIndex: record.index,
      textId: text.id,
      rect: text.rect,
    });
  }

  private showBoundHover(record: PageRecord, bound: BoundInteraction) {
    const key = `bound:${record.index}:${bound.kind}:${bound.rect.x}:${bound.rect.y}:${bound.rect.width}:${bound.rect.height}`;
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = undefined;
    record.container.style.cursor = "";
    this.renderInteractionHighlights(record, [bound.rect], "bound");
  }

  private requestBoundHover(record: PageRecord, point: { x: number; y: number }) {
    const key = `bound-query:${record.index}:${point.x.toFixed(1)}:${point.y.toFixed(1)}`;
    if (this.activeHoverKey === key) {
      return;
    }

    const request = {
      requestId: ++this.nextHitRequestId,
      generation: this.interactionGeneration,
      pageIndex: record.index,
      x: point.x,
      y: point.y,
    };
    this.activeHoverKey = key;
    this.pendingBoundHover = request;
    record.container.style.cursor = "";
    this.postWorker({
      type: "hit-bound",
      ...request,
    });
  }

  private renderLinkAnchors(record: PageRecord) {
    const interactions = record.interactions;
    if (!interactions) {
      record.linkLayer.replaceChildren();
      return;
    }

    const anchors = interactions.links.flatMap((link) => {
      if (link.target.kind !== "external") {
        return [];
      }

      const anchor = document.createElement("a");
      anchor.href = link.target.href;
      anchor.target = "_blank";
      anchor.rel = "noopener noreferrer";
      anchor.title = link.target.href;
      anchor.setAttribute("aria-label", link.target.href);
      anchor.addEventListener("click", (event) => event.stopPropagation());
      this.applyRectStyle(record, anchor, link.rect);
      return [anchor];
    });

    record.linkLayer.replaceChildren(...anchors);
  }

  private renderInteractionHighlights(
    record: PageRecord,
    rects: PageRect[],
    kind: "link" | "text" | "bound",
  ) {
    record.interactionLayer.replaceChildren(
      ...rects.slice(0, 64).map((rect) => {
        const highlight = document.createElement("div");
        highlight.className = `typst-interaction-highlight ${kind}`;
        this.applyRectStyle(record, highlight, rect);
        return highlight;
      }),
    );
  }

  private clearInteractionHighlight(record: PageRecord) {
    record.container.style.cursor = "";
    this.activeTextHover = undefined;
    record.interactionLayer.replaceChildren();
  }

  private activeTextHoverContains(record: PageRecord, point: { x: number; y: number }) {
    return (
      this.activeTextHover?.generation === this.interactionGeneration &&
      this.activeTextHover.pageIndex === record.index &&
      rectContainsPage(this.activeTextHover.rect, point.x, point.y)
    );
  }

  private cachedTextHoverAt(record: PageRecord, point: { x: number; y: number }) {
    const texts = record.interactions?.texts;
    if (!texts) {
      return undefined;
    }
    for (let index = texts.length - 1; index >= 0; index -= 1) {
      const text = texts[index];
      const rect = this.textHoverRects.get(this.textHoverKey(record.index, text.id));
      if (rect && rectContainsPage(rect, point.x, point.y)) {
        return { text, rect };
      }
    }
    return undefined;
  }

  private textHoverKey(pageIndex: number, textId: number) {
    return `${this.interactionGeneration}:${pageIndex}:${textId}`;
  }

  private textHitContains(text: TextInteraction, hitRect: PageRect | undefined, x: number, y: number) {
    return rectContainsPage(textHoverRect(text, hitRect), x, y);
  }

  private applyRectStyle(record: PageRecord, element: HTMLElement, rect: PageRect) {
    const x = clamp(rect.x / Math.max(record.width, 1), 0, 1);
    const y = clamp(rect.y / Math.max(record.height, 1), 0, 1);
    const right = clamp((rect.x + rect.width) / Math.max(record.width, 1), 0, 1);
    const bottom = clamp((rect.y + rect.height) / Math.max(record.height, 1), 0, 1);
    element.style.left = `${x * 100}%`;
    element.style.top = `${y * 100}%`;
    element.style.width = `${Math.max(0, right - x) * 100}%`;
    element.style.height = `${Math.max(0, bottom - y) * 100}%`;
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

  private applyPageLayout(record: PageRecord, page: PageSpec, metrics: PageLayoutMetrics) {
    record.width = page.width;
    record.height = page.height;
    record.pixelPerPt = page.pixelPerPt;
    record.container.hidden =
      this.previewMode === "Slide" && page.index !== this.currentSlidePage - 1;
    const scale = this.computePageScale(page, metrics);
    const cssWidth = Math.ceil(page.width * scale);
    const cssHeight = Math.ceil(page.height * scale);
    record.cssWidth = cssWidth;
    record.cssHeight = cssHeight;
    record.container.style.width = `${cssWidth}px`;
    record.container.style.height = `${cssHeight}px`;
    record.shell.style.width = `${cssWidth}px`;
    record.shell.style.height = `${cssHeight}px`;
    record.shell.dataset.appliedScale = String(scale);
    this.alignCanvasBackingStore(record, page);
  }

  private alignCanvasBackingStore(record: PageRecord, page: PageSpec) {
    const drawnWidthPx = page.width * page.pixelPerPt;
    const drawnHeightPx = page.height * page.pixelPerPt;
    const widthScale = record.fullWidthPx / Math.max(drawnWidthPx, 1);
    const heightScale = record.fullHeightPx / Math.max(drawnHeightPx, 1);
    record.canvas.style.width = `${widthScale * 100}%`;
    record.canvas.style.height = `${heightScale * 100}%`;
  }

  private computePageScale(page: PageSpec, metrics: PageLayoutMetrics): number {
    const fitWidth = metrics.availableWidth / page.width;
    const baseScale =
      this.previewMode === "Slide"
        ? Math.max(0.1, Math.min(fitWidth, metrics.availableHeight / page.height))
        : Math.max(0.1, fitWidth);
    return baseScale * this.zoomRatio;
  }

  private readPageLayoutMetrics(): PageLayoutMetrics {
    return {
      availableWidth: Math.max(1, this.elements.viewport.clientWidth),
      availableHeight: Math.max(1, this.elements.viewport.clientHeight),
    };
  }

  private pageGap() {
    return this.contentPreview ? 5 : 10;
  }

  private zoomAnchorFromEvent(
    event: WheelEvent,
    metrics: PageLayoutMetrics,
    layouts: PageLayoutRecord[],
  ): ZoomAnchor | undefined {
    const viewportRect = this.elements.viewport.getBoundingClientRect();
    const viewportX = event.clientX - viewportRect.left;
    const viewportY = event.clientY - viewportRect.top;
    const contentX = this.elements.viewport.scrollLeft + viewportX;
    const contentY = this.elements.viewport.scrollTop + viewportY;
    const layout = this.findPageLayoutAt(layouts, contentY) || this.nearestPageLayout(layouts, contentY);
    if (!layout) {
      return undefined;
    }

    const record = this.pageRecords.get(layout.index);
    if (!record) {
      return undefined;
    }

    const pageLeft = this.pageLeft(record, metrics);
    return {
      pageIndex: layout.index,
      pageX: clamp((contentX - pageLeft) / Math.max(layout.scaleX, 1e-6), 0, record.width),
      pageY: clamp((contentY - layout.top) / Math.max(layout.scaleY, 1e-6), 0, record.height),
      viewportX,
      viewportY,
    };
  }

  private restoreZoomAnchor(
    anchor: ZoomAnchor | undefined,
    metrics: PageLayoutMetrics,
    layouts: PageLayoutRecord[],
  ) {
    if (!anchor) {
      return;
    }

    const record = this.pageRecords.get(anchor.pageIndex);
    const layout = layouts.find((candidate) => candidate.index === anchor.pageIndex);
    if (!record || !layout) {
      return;
    }

    const pageLeft = this.pageLeft(record, metrics);
    this.elements.viewport.scrollTo({
      left: pageLeft + anchor.pageX * layout.scaleX - anchor.viewportX,
      top: layout.top + anchor.pageY * layout.scaleY - anchor.viewportY,
      behavior: "auto",
    });
  }

  private findPageLayoutAt(layouts: PageLayoutRecord[], contentY: number) {
    return layouts.find((layout) => contentY >= layout.top && contentY <= layout.bottom);
  }

  private nearestPageLayout(layouts: PageLayoutRecord[], contentY: number) {
    let nearest: PageLayoutRecord | undefined;
    let nearestDistance = Number.POSITIVE_INFINITY;
    for (const layout of layouts) {
      const distance =
        contentY < layout.top
          ? layout.top - contentY
          : contentY > layout.bottom
            ? contentY - layout.bottom
            : 0;
      if (distance < nearestDistance) {
        nearestDistance = distance;
        nearest = layout;
      }
    }
    return nearest;
  }

  private pageLeft(record: PageRecord, metrics: PageLayoutMetrics) {
    if (this.contentPreview) {
      return 0;
    }
    return (metrics.availableWidth - record.cssWidth) / 2;
  }

  private handleJumpMessage(text: string) {
    const positions = parsePreviewPositions(text);
    if (positions.length === 0) {
      return;
    }

    const currentPage =
      this.previewMode === "Slide" ? this.currentSlidePage : this.currentViewportPage();
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
    const [page, x, y] = text
      .trim()
      .split(/\s+/)
      .map((value) => Number.parseFloat(value));
    if (!Number.isFinite(page) || !Number.isFinite(x) || !Number.isFinite(y)) {
      return;
    }

    this.pendingCursor = { page, x, y };
    if (this.previewMode === "Slide") {
      this.setCurrentSlidePage(page);
    }
    this.renderCursor();
  }

  private scrollToTypstLocation(position: PreviewPosition) {
    const record = this.pageRecords.get(position.page - 1);
    if (!record) {
      return;
    }

    const xRatio = clamp(position.x / Math.max(record.width, 1), 0, 1);
    const yRatio = clamp(position.y / Math.max(record.height, 1), 0, 1);
    const left = record.container.offsetLeft + xRatio * record.container.offsetWidth;
    const top = record.container.offsetTop + yRatio * record.container.offsetHeight;
    this.elements.viewport.scrollTo({
      left: Math.max(0, left - this.elements.viewport.clientWidth / 2),
      top: Math.max(0, top - this.elements.viewport.clientHeight / 2),
      behavior: "auto",
    });
    this.showJumpMarker(record, xRatio, yRatio);
    this.scheduleViewportSnapshot();
  }

  private showJumpMarker(record: PageRecord, xRatio: number, yRatio: number) {
    record.jumpMarker.style.left = `${xRatio * 100}%`;
    record.jumpMarker.style.top = `${yRatio * 100}%`;
    record.jumpMarker.classList.remove("visible");
    void record.jumpMarker.offsetWidth;
    record.jumpMarker.classList.add("visible");
    window.setTimeout(() => record.jumpMarker.classList.remove("visible"), 900);
  }

  private renderCursor() {
    for (const record of this.pageRecords.values()) {
      record.cursor.classList.remove("visible");
    }
    if (!this.pendingCursor) {
      return;
    }

    const record = this.pageRecords.get(this.pendingCursor.page - 1);
    if (!record) {
      return;
    }

    record.cursor.style.left = `${
      clamp(this.pendingCursor.x / Math.max(record.width, 1), 0, 1) * 100
    }%`;
    record.cursor.style.top = `${
      clamp(this.pendingCursor.y / Math.max(record.height, 1), 0, 1) * 100
    }%`;
    record.cursor.classList.add("visible");
  }

  private currentViewportPage(): number {
    const viewportCenter =
      this.elements.viewport.scrollTop + this.elements.viewport.clientHeight / 2;
    let bestPage = 1;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (const layout of this.collectPageLayouts()) {
      const center = (layout.top + layout.bottom) / 2;
      const distance = Math.abs(center - viewportCenter);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestPage = layout.index + 1;
      }
    }
    return bestPage;
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
        this.applyPageLayout(record, page, metrics);
      }
    }
    this.renderCursor();
  }

  private renderOutline() {
    const items = Array.isArray(this.outlineData?.items) ? this.outlineData.items : [];
    this.elements.outlinePanel.replaceChildren();
    this.elements.outlinePanel.classList.toggle(
      "hidden",
      !this.contentPreview || items.length === 0,
    );
    if (!this.contentPreview || items.length === 0) {
      return;
    }

    const fragment = document.createDocumentFragment();
    for (const item of items) {
      fragment.appendChild(this.createOutlineItem(item, 1));
    }
    this.elements.outlinePanel.appendChild(fragment);
  }

  private createOutlineItem(item: any, level: number): HTMLElement {
    const container = document.createElement("div");
    container.className = `typst-outline level-${Math.min(level, 5)}`;
    const title = document.createElement("button");
    title.type = "button";
    title.className = "typst-outline-title";
    title.textContent = String(item?.title ?? "");
    const position = item?.position;
    if (position) {
      title.addEventListener("click", () => {
        this.scrollToTypstLocation({
          page: Number(position.page_no ?? position.page ?? 1),
          x: Number(position.x ?? 0),
          y: Number(position.y ?? 0),
        });
      });
    }
    container.appendChild(title);
    for (const child of Array.isArray(item?.children) ? item.children : []) {
      container.appendChild(this.createOutlineItem(child, level + 1));
    }
    return container;
  }

  private ensureInvertColors(strategy: InvertColorStrategy | InvertColorStrategyMap) {
    const target = typeof strategy === "string" ? { rest: strategy } : strategy;
    const decide = (value: InvertColorStrategy | undefined) => {
      switch (value || "never") {
        case "always":
          return true;
        case "auto":
          return (this.autoInvertDecision ??= determineInvertColor());
        case "never":
        default:
          return false;
      }
    };
    this.elements.root.classList.toggle("invert-colors", decide(target.rest));
    this.elements.root.classList.toggle("normal-image", !decide(target.image || target.rest));
  }
}

function determineInvertColor() {
  const cls = document.body.classList;
  return (
    (cls.contains("vscode-dark") || cls.contains("vscode-high-contrast")) &&
    !cls.contains("vscode-light")
  );
}

function textHoverRect(text: TextInteraction, hitRect: PageRect | undefined) {
  return hitRect || text.rect;
}

function rectContainsPage(rect: PageRect, x: number, y: number) {
  return x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height;
}

function alignTextClipToVisualRect(clippedRect: PageRect, visualRect: PageRect): PageRect {
  return {
    x: clippedRect.x,
    y: visualRect.y,
    width: clippedRect.width,
    height: visualRect.height,
  };
}

const zoomFactors = [
  0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9, 2.1, 2.4, 2.7, 3, 3.3,
  3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
];
