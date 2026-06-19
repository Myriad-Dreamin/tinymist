export const SVG_SCALE_EPSILON = 1e-6;

export interface SvgResizePageAnchor {
  kind: "page";
  pageNumber: number;
  pageLocalY: number;
  pageHeight: number;
}

export interface SvgResizeGapAnchor {
  kind: "gap";
  beforePageNumber?: number;
  afterPageNumber?: number;
  gapRatio: number;
}

export type SvgResizeAnchor = SvgResizePageAnchor | SvgResizeGapAnchor;

export interface SvgAnchorPage {
  pageNumber: number;
  y: number;
  height: number;
}

const finite = (value: number | undefined): value is number =>
  value !== undefined && Number.isFinite(value);

export function hasSvgScaleRatioChanged(
  previousScaleRatio: number | undefined,
  currentScaleRatio: number,
  epsilon = SVG_SCALE_EPSILON,
) {
  return (
    previousScaleRatio !== undefined && Math.abs(currentScaleRatio - previousScaleRatio) >= epsilon
  );
}

export function resolveViewportStart(
  scroll: number | undefined,
  contentOffset: number | undefined,
  appliedScale: number | undefined,
) {
  if (!finite(scroll) || !finite(contentOffset) || !finite(appliedScale) || appliedScale <= 0) {
    return undefined;
  }

  return (scroll - contentOffset) / appliedScale;
}

export function resolveScrollForViewportStart(
  viewportStart: number,
  appliedScale: number,
  contentOffset: number,
  scrollSize?: number,
  clientSize?: number,
) {
  if (!Number.isFinite(viewportStart) || !Number.isFinite(appliedScale) || appliedScale <= 0) {
    return undefined;
  }
  if (!Number.isFinite(contentOffset)) {
    return undefined;
  }

  const target = viewportStart * appliedScale + contentOffset;
  if (!Number.isFinite(target)) {
    return undefined;
  }

  if (finite(scrollSize) && finite(clientSize)) {
    const max = Math.max(0, scrollSize - clientSize);
    return Math.max(0, Math.min(max, target));
  }

  return Math.max(0, target);
}

export function captureSvgResizeYAnchor(
  pages: SvgAnchorPage[],
  svgHeight: number,
  viewportTopY: number,
): SvgResizeAnchor | undefined {
  if (!Number.isFinite(svgHeight) || svgHeight <= 0 || !Number.isFinite(viewportTopY)) {
    return undefined;
  }

  const containingPage = pages.find(
    (page) => viewportTopY >= page.y && viewportTopY <= page.y + page.height,
  );
  if (containingPage) {
    return {
      kind: "page",
      pageNumber: containingPage.pageNumber,
      pageLocalY: viewportTopY - containingPage.y,
      pageHeight: containingPage.height,
    };
  }

  let beforePage: SvgAnchorPage | undefined;
  let afterPage: SvgAnchorPage | undefined;
  for (const page of pages) {
    if (viewportTopY < page.y) {
      afterPage = page;
      break;
    }
    if (viewportTopY > page.y + page.height) {
      beforePage = page;
    }
  }

  const gapStartY = beforePage ? beforePage.y + beforePage.height : 0;
  const gapEndY = afterPage ? afterPage.y : svgHeight;
  if (
    !Number.isFinite(gapStartY) ||
    !Number.isFinite(gapEndY) ||
    gapEndY < gapStartY ||
    viewportTopY < gapStartY ||
    viewportTopY > gapEndY
  ) {
    return undefined;
  }

  const gapLength = gapEndY - gapStartY;
  const gapAnchor: SvgResizeGapAnchor = {
    kind: "gap",
    gapRatio: gapLength > 0 ? Math.max(0, Math.min(1, (viewportTopY - gapStartY) / gapLength)) : 0,
  };
  if (beforePage) {
    gapAnchor.beforePageNumber = beforePage.pageNumber;
  }
  if (afterPage) {
    gapAnchor.afterPageNumber = afterPage.pageNumber;
  }
  return gapAnchor;
}

export function resolveSyntheticYForResizeAnchor(
  anchor: SvgResizeAnchor,
  pages: SvgAnchorPage[],
  svgHeight: number,
) {
  if (!Number.isFinite(svgHeight) || svgHeight <= 0) {
    return undefined;
  }

  if (anchor.kind === "gap") {
    const beforePage =
      anchor.beforePageNumber !== undefined
        ? pages.find((page) => page.pageNumber === anchor.beforePageNumber)
        : undefined;
    const afterPage =
      anchor.afterPageNumber !== undefined
        ? pages.find((page) => page.pageNumber === anchor.afterPageNumber)
        : undefined;

    if (
      (anchor.beforePageNumber !== undefined && beforePage === undefined) ||
      (anchor.afterPageNumber !== undefined && afterPage === undefined)
    ) {
      return undefined;
    }

    const gapStartY = beforePage ? beforePage.y + beforePage.height : 0;
    const gapEndY = afterPage ? afterPage.y : svgHeight;
    if (!Number.isFinite(gapStartY) || !Number.isFinite(gapEndY) || gapEndY < gapStartY) {
      return undefined;
    }

    const gapRatio = Math.max(0, Math.min(1, anchor.gapRatio));
    return gapStartY + gapRatio * (gapEndY - gapStartY);
  }

  const page = pages.find((page) => page.pageNumber === anchor.pageNumber);
  if (!page || anchor.pageHeight <= 0) {
    return undefined;
  }

  return page.y + (anchor.pageLocalY * page.height) / anchor.pageHeight;
}
