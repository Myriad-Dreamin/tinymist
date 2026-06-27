import type { PageLayout, PageRenderSpec, PageSpec } from "./types";

const minPixelPerPt = 0.5;
const maxPixelPerPt = 4;
const interactivePixelPerPtScale = 0.65;
const minInteractivePixelPerPt = 0.7;
const maxInteractivePixelPerPt = 1.25;

export function yieldToEventLoop(): Promise<void> {
  const scheduler = (globalThis as any).scheduler;
  if (typeof scheduler?.yield === "function") {
    return scheduler.yield();
  }
  return new Promise((resolve) => setTimeout(resolve, 0));
}

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function clampPixelPerPt(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return minPixelPerPt;
  }
  const bucketed = Math.ceil((value - 1e-3) * 4) / 4;
  return clamp(bucketed, minPixelPerPt, maxPixelPerPt);
}

export function computeInteractivePixelPerPt(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return minInteractivePixelPerPt;
  }
  return clamp(
    value * interactivePixelPerPtScale,
    minInteractivePixelPerPt,
    maxInteractivePixelPerPt,
  );
}

export function isFullPageRender(page: PageRenderSpec) {
  const window = page.window;
  if (!window) {
    return true;
  }
  return (
    window.lo.x <= 0 && window.lo.y <= 0 && window.hi.x >= page.width && window.hi.y >= page.height
  );
}

export function commitPixelRect(
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

export function renderWindowKey(page: PageRenderSpec) {
  if (!page.window) {
    return "full";
  }
  const { lo, hi } = page.window;
  return `${lo.x}:${lo.y}:${hi.x}:${hi.y}`;
}

export function pageDistanceToViewport(page: PageRenderSpec, layouts: PageLayout[], viewport: any) {
  const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || 1);
  const viewportTop = Math.max(0, viewport.scrollTop || 0);
  const viewportBottom = viewportTop + viewportHeight;
  const viewportCenter = (viewportTop + viewportBottom) / 2;
  const layout = layouts.find((candidate) => candidate.index === page.index);
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

export function selectStagePagesAt(
  pages: PageSpec[],
  layouts: PageLayout[],
  viewport: any,
  screens: number,
  viewportTop: number,
  options: { windowed?: boolean } = {},
): PageRenderSpec[] {
  if (!Number.isFinite(screens)) {
    return pages.map((page) => ({ ...page }));
  }

  const viewportHeight = Math.max(1, viewport.height || viewport.window?.innerHeight || 1);
  const viewportBottom = viewportTop + viewportHeight;
  const sideScreens = Math.max(0, (screens - 1) / 2);
  const rangeTop = Math.max(0, viewportTop - viewportHeight * sideScreens);
  const rangeBottom = viewportBottom + viewportHeight * sideScreens;
  const viewportCenter = (viewportTop + viewportBottom) / 2;
  const layoutMap = new Map(layouts.map((layout) => [layout.index, layout]));

  return pages
    .flatMap((page) => {
      const layout = layoutMap.get(page.index);
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
