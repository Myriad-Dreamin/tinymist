import type { PageLayout, PageRecord, PageSpec, PreviewMode, ViewportSnapshot } from "./types";
import { clamp } from "./utils";

export interface PageLayoutMetrics {
  availableWidth: number;
  availableHeight: number;
}

export interface PageLayoutRecord extends PageLayout {
  scaleX: number;
  scaleY: number;
}

export interface ZoomAnchor {
  pageIndex: number;
  pageX: number;
  pageY: number;
  viewportX: number;
  viewportY: number;
}

interface PageLayoutOptions {
  previewMode: PreviewMode;
  currentSlidePage: number;
  zoomRatio: number;
}

const zoomFactors = [
  0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9, 2.1, 2.4, 2.7, 3, 3.3,
  3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
];

export function nextZoomRatio(current: number, deltaY: number) {
  if (deltaY < 0) {
    return zoomFactors.find((factor) => factor > current) ?? current;
  }
  if (deltaY > 0) {
    return [...zoomFactors].reverse().find((factor) => factor < current) ?? current;
  }
  return current;
}

export function collectPageLayouts(
  pages: PageSpec[],
  records: ReadonlyMap<number, PageRecord>,
  gap: number,
): PageLayoutRecord[] {
  const layouts: PageLayoutRecord[] = [];
  let top = 0;
  let visibleCount = 0;

  for (const page of pages) {
    const record = records.get(page.index);
    if (!record || record.container.hidden) {
      continue;
    }

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
  }

  return layouts;
}

export function readPageLayoutMetrics(viewport: HTMLElement): PageLayoutMetrics {
  return {
    availableWidth: Math.max(1, viewport.clientWidth),
    availableHeight: Math.max(1, viewport.clientHeight),
  };
}

export function readViewportSnapshot(
  viewport: HTMLElement,
  metrics: PageLayoutMetrics,
  state: { dragging: boolean; scrolling: boolean },
): ViewportSnapshot {
  const width = metrics.availableWidth;
  const height = metrics.availableHeight;
  return {
    width,
    height,
    scrollLeft: viewport.scrollLeft,
    scrollTop: viewport.scrollTop,
    devicePixelRatio: window.devicePixelRatio || 1,
    dragging: state.dragging,
    scrolling: state.scrolling,
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

export function viewportPostKey(viewport: ViewportSnapshot) {
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

export function applyPageLayout(
  record: PageRecord,
  page: PageSpec,
  metrics: PageLayoutMetrics,
  options: PageLayoutOptions,
) {
  record.width = page.width;
  record.height = page.height;
  record.pixelPerPt = page.pixelPerPt;
  record.container.hidden =
    options.previewMode === "Slide" && page.index !== options.currentSlidePage - 1;
  const scale = computePageScale(page, metrics, options.previewMode, options.zoomRatio);
  const cssWidth = Math.ceil(page.width * scale);
  const cssHeight = Math.ceil(page.height * scale);
  record.cssWidth = cssWidth;
  record.cssHeight = cssHeight;
  record.container.style.width = `${cssWidth}px`;
  record.container.style.height = `${cssHeight}px`;
  record.shell.style.width = `${cssWidth}px`;
  record.shell.style.height = `${cssHeight}px`;
  record.shell.dataset.appliedScale = String(scale);
  alignCanvasBackingStore(record, page);
}

export function currentViewportPage(viewport: HTMLElement, layouts: PageLayoutRecord[]): number {
  const viewportCenter = viewport.scrollTop + viewport.clientHeight / 2;
  let bestPage = 1;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const layout of layouts) {
    const center = (layout.top + layout.bottom) / 2;
    const distance = Math.abs(center - viewportCenter);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestPage = layout.index + 1;
    }
  }
  return bestPage;
}

export function zoomAnchorFromEvent(
  event: WheelEvent,
  viewport: HTMLElement,
  records: ReadonlyMap<number, PageRecord>,
  metrics: PageLayoutMetrics,
  layouts: PageLayoutRecord[],
  contentPreview: boolean,
): ZoomAnchor | undefined {
  const viewportRect = viewport.getBoundingClientRect();
  const viewportX = event.clientX - viewportRect.left;
  const viewportY = event.clientY - viewportRect.top;
  const contentX = viewport.scrollLeft + viewportX;
  const contentY = viewport.scrollTop + viewportY;
  const layout = findPageLayoutAt(layouts, contentY) || nearestPageLayout(layouts, contentY);
  if (!layout) {
    return undefined;
  }

  const record = records.get(layout.index);
  if (!record) {
    return undefined;
  }

  const left = pageLeft(record, metrics, contentPreview);
  return {
    pageIndex: layout.index,
    pageX: clamp((contentX - left) / Math.max(layout.scaleX, 1e-6), 0, record.width),
    pageY: clamp((contentY - layout.top) / Math.max(layout.scaleY, 1e-6), 0, record.height),
    viewportX,
    viewportY,
  };
}

export function restoreZoomAnchor(
  anchor: ZoomAnchor | undefined,
  viewport: HTMLElement,
  records: ReadonlyMap<number, PageRecord>,
  metrics: PageLayoutMetrics,
  layouts: PageLayoutRecord[],
  contentPreview: boolean,
) {
  if (!anchor) {
    return;
  }

  const record = records.get(anchor.pageIndex);
  const layout = layouts.find((candidate) => candidate.index === anchor.pageIndex);
  if (!record || !layout) {
    return;
  }

  const left = pageLeft(record, metrics, contentPreview);
  viewport.scrollTo({
    left: left + anchor.pageX * layout.scaleX - anchor.viewportX,
    top: layout.top + anchor.pageY * layout.scaleY - anchor.viewportY,
    behavior: "auto",
  });
}

function alignCanvasBackingStore(record: PageRecord, page: PageSpec) {
  const drawnWidthPx = page.width * page.pixelPerPt;
  const drawnHeightPx = page.height * page.pixelPerPt;
  const widthScale = record.fullWidthPx / Math.max(drawnWidthPx, 1);
  const heightScale = record.fullHeightPx / Math.max(drawnHeightPx, 1);
  record.canvas.style.width = `${widthScale * 100}%`;
  record.canvas.style.height = `${heightScale * 100}%`;
}

function computePageScale(
  page: PageSpec,
  metrics: PageLayoutMetrics,
  previewMode: PreviewMode,
  zoomRatio: number,
): number {
  const fitWidth = metrics.availableWidth / page.width;
  const baseScale =
    previewMode === "Slide"
      ? Math.max(0.1, Math.min(fitWidth, metrics.availableHeight / page.height))
      : Math.max(0.1, fitWidth);
  return baseScale * zoomRatio;
}

function pageLeft(record: PageRecord, metrics: PageLayoutMetrics, contentPreview: boolean) {
  if (contentPreview) {
    return 0;
  }
  return (metrics.availableWidth - record.cssWidth) / 2;
}

function findPageLayoutAt(layouts: PageLayoutRecord[], contentY: number) {
  return layouts.find((layout) => contentY >= layout.top && contentY <= layout.bottom);
}

function nearestPageLayout(layouts: PageLayoutRecord[], contentY: number) {
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
