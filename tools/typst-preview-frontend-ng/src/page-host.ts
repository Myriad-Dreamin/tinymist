import type { PreviewElements } from "./dom";
import { installPageControls } from "./controls";
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
  setLastMessage: (message: string) => void;
}

/** Owns preview page DOM and controls, handling render-worker page requests and routed preview protocol messages. */
export class PageHost {
  private readonly elements: PreviewElements;
  private readonly postWorker: PageHostOptions["postWorker"];
  private readonly setLastMessage: PageHostOptions["setLastMessage"];
  private previewMode: PreviewMode = "Doc";
  private contentPreview = false;
  private currentSlidePage = 1;
  private pageCount = 0;
  private zoomRatio = 1;
  private lastViewportTimer = 0;
  private lastPages: PageSpec[] = [];
  private pendingCursor: PreviewPosition | undefined;
  private outlineData: any;
  private autoInvertDecision: boolean | undefined;
  private readonly pageRecords = new Map<number, PageRecord>();

  constructor({ elements, postWorker, setLastMessage }: PageHostOptions) {
    this.elements = elements;
    this.postWorker = postWorker;
    this.setLastMessage = setLastMessage;
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

  setSlidePageFromInput(value: string) {
    if (value.trim().length === 0) {
      return;
    }
    this.setCurrentSlidePage(Number.parseInt(value, 10));
  }

  focusPageSelector() {
    this.elements.pageSelector.focus();
  }

  blurPageSelector() {
    this.elements.pageSelector.blur();
  }

  hideHelp() {
    this.elements.helpPanel.classList.add("hidden");
  }

  toggleHelp() {
    this.elements.helpPanel.classList.toggle("hidden");
  }

  toggleInvertColors() {
    this.elements.root.classList.toggle("invert-colors");
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

    const rect = this.elements.viewport.getBoundingClientRect();
    const anchorX = this.elements.viewport.scrollLeft + event.clientX - rect.left;
    const anchorY = this.elements.viewport.scrollTop + event.clientY - rect.top;
    this.applyAllPageLayouts();
    const ratio = this.zoomRatio / previousZoom;
    this.elements.viewport.scrollBy(anchorX * (ratio - 1), anchorY * (ratio - 1));
    this.scheduleViewportSnapshot();
  }

  setPreviewMode(mode: PreviewMode) {
    this.previewMode = mode;
    this.elements.previewMode.textContent = mode;
    this.elements.root.classList.toggle("mode-slide", mode === "Slide");
    this.elements.root.classList.toggle("mode-doc", mode === "Doc");
    this.elements.helpPanel.classList.toggle("mode-slide", mode === "Slide");
    this.elements.helpPanel.classList.toggle("mode-doc", mode === "Doc");
    this.updatePageControls();
    this.applyAllPageLayouts();
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

  resetCounters() {
    this.elements.frameCount.textContent = "frames: 0";
    this.elements.byteCount.textContent = "bytes: 0";
    this.elements.renderCount.textContent = "renders: 0";
  }

  ensurePages(generation: number, pages: PageSpec[]) {
    if (!("transferControlToOffscreen" in HTMLCanvasElement.prototype)) {
      throw new Error("OffscreenCanvas transfer is not supported in this browser");
    }

    this.lastPages = pages;
    this.updatePageCount(pages.length);
    this.elements.emptyState.classList.toggle("hidden", pages.length > 0);

    const livePages = new Set<number>();
    const transferred: Array<{
      index: number;
      canvas: OffscreenCanvas;
      widthPx: number;
      heightPx: number;
    }> = [];

    for (const page of pages) {
      livePages.add(page.index);
      const widthPx = Math.max(1, Math.ceil(page.width * page.pixelPerPt));
      const heightPx = Math.max(1, Math.ceil(page.height * page.pixelPerPt));
      const key = `${page.width.toFixed(3)}:${page.height.toFixed(3)}:${page.pixelPerPt}`;
      let record = this.pageRecords.get(page.index);

      if (!record || record.key !== key) {
        record?.container.remove();
        record = this.createPageRecord(page, key, widthPx, heightPx);
        this.pageRecords.set(page.index, record);
        this.insertPage(record, page.index);
      }

      this.applyPageLayout(record, page);
      if (!record.transferred) {
        record.canvas.width = widthPx;
        record.canvas.height = heightPx;
        const offscreen = record.canvas.transferControlToOffscreen();
        record.transferred = true;
        transferred.push({ index: page.index, canvas: offscreen, widthPx, heightPx });
      }
    }

    for (const [index, record] of this.pageRecords) {
      if (!livePages.has(index)) {
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

  collectPageLayouts() {
    const viewportRect = this.elements.viewport.getBoundingClientRect();
    const scrollTop = this.elements.viewport.scrollTop;
    return [...this.pageRecords.entries()]
      .sort(([a], [b]) => a - b)
      .flatMap(([index, record]) => {
        if (record.container.hidden) {
          return [];
        }
        const rect = record.container.getBoundingClientRect();
        const top = rect.top - viewportRect.top + scrollTop;
        const height = rect.height;
        return [
          {
            index,
            top,
            bottom: top + height,
            height,
            scale: height / Math.max(record.height, 1),
          },
        ];
      });
  }

  readViewportSnapshot(): ViewportSnapshot {
    const viewport = this.elements.viewport;
    const rect = viewport.getBoundingClientRect();
    return {
      width: viewport.clientWidth,
      height: viewport.clientHeight,
      scrollLeft: viewport.scrollLeft,
      scrollTop: viewport.scrollTop,
      devicePixelRatio: window.devicePixelRatio || 1,
      window: {
        innerWidth: window.innerWidth,
        innerHeight: window.innerHeight,
      },
      boundingRect: {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        left: rect.left,
      },
    };
  }

  scheduleViewportSnapshot() {
    if (this.lastViewportTimer) {
      clearTimeout(this.lastViewportTimer);
    }
    this.lastViewportTimer = window.setTimeout(() => {
      this.lastViewportTimer = 0;
      this.applyAllPageLayouts();
      this.postWorker({
        type: "viewport",
        viewport: this.readViewportSnapshot(),
        layouts: this.collectPageLayouts(),
      });
    }, 50);
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
        this.setLastMessage("cursor paths ignored");
        break;
      case "partial-rendering":
        this.setLastMessage("partial rendering enabled");
        break;
      case "invert-colors":
        this.ensureInvertColors(parseInvertColorStrategy(text));
        break;
      case "outline":
        try {
          this.outlineData = JSON.parse(text);
          this.renderOutline();
        } catch (_error) {
          this.setLastMessage("outline received");
        }
        break;
    }
  }

  clearPages() {
    for (const record of this.pageRecords.values()) {
      record.container.remove();
    }
    this.pageRecords.clear();
    this.lastPages = [];
    this.pendingCursor = undefined;
    this.updatePageCount(0);
    this.elements.emptyState.classList.remove("hidden");
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
    canvas.width = widthPx;
    canvas.height = heightPx;

    const cursor = document.createElement("div");
    cursor.className = "typst-cursor";

    const jumpMarker = document.createElement("div");
    jumpMarker.className = "typst-jump-marker";

    container.addEventListener("click", () => {
      if (this.contentPreview) {
        this.postWorker({ type: "send", text: `outline-sync,${page.index + 1}` });
      }
    });

    shell.appendChild(canvas);
    container.append(shell, cursor, jumpMarker);

    return {
      key,
      container,
      shell,
      canvas,
      cursor,
      jumpMarker,
      transferred: false,
      width: page.width,
      height: page.height,
      pixelPerPt: page.pixelPerPt,
    };
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

  private applyPageLayout(record: PageRecord, page: PageSpec) {
    record.width = page.width;
    record.height = page.height;
    record.pixelPerPt = page.pixelPerPt;
    record.container.hidden =
      this.previewMode === "Slide" && page.index !== this.currentSlidePage - 1;
    const scale = this.computePageScale(page);
    const cssWidth = Math.ceil(page.width * scale);
    const cssHeight = Math.ceil(page.height * scale);
    record.container.style.width = `${cssWidth}px`;
    record.container.style.height = `${cssHeight}px`;
    record.shell.style.width = `${cssWidth}px`;
    record.shell.style.height = `${cssHeight}px`;
    record.shell.dataset.appliedScale = String(scale);
  }

  private computePageScale(page: PageSpec): number {
    const availableWidth = Math.max(1, this.elements.viewport.clientWidth - 48);
    const availableHeight = Math.max(1, this.elements.viewport.clientHeight - 48);
    const fitWidth = availableWidth / page.width;
    const baseScale =
      this.previewMode === "Slide"
        ? Math.max(0.1, Math.min(fitWidth, availableHeight / page.height))
        : Math.max(0.1, fitWidth);
    return baseScale * this.zoomRatio;
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
    for (const [index, record] of this.pageRecords) {
      if (record.container.hidden) {
        continue;
      }
      const center = record.container.offsetTop + record.container.offsetHeight / 2;
      const distance = Math.abs(center - viewportCenter);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestPage = index + 1;
      }
    }
    return bestPage;
  }

  private setCurrentSlidePage(page: number) {
    if (!Number.isFinite(page) || this.pageCount === 0) {
      return;
    }
    this.currentSlidePage = clamp(Math.trunc(page), 1, this.pageCount);
    this.updatePageControls();
    this.applyAllPageLayouts();
    this.renderCursor();
    this.elements.viewport.scrollTo({ top: 0, left: 0, behavior: "auto" });
    this.scheduleViewportSnapshot();
  }

  private updatePageCount(nextPageCount: number) {
    this.pageCount = nextPageCount;
    this.currentSlidePage = clamp(this.currentSlidePage, 1, Math.max(1, this.pageCount));
    this.updatePageControls();
  }

  private updatePageControls() {
    this.elements.pageSelector.value = String(this.currentSlidePage);
    this.elements.pageTotal.textContent = String(this.pageCount);
    this.elements.pageSelector.style.setProperty(
      "--page-length-digits",
      String(String(this.pageCount).length),
    );
  }

  private applyAllPageLayouts() {
    for (const page of this.lastPages) {
      const record = this.pageRecords.get(page.index);
      if (record) {
        this.applyPageLayout(record, page);
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

const zoomFactors = [
  0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9, 2.1, 2.4, 2.7, 3, 3.3,
  3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
];
