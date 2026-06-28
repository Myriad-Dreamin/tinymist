import type { PageRecord, PageSpec } from "./types";

export function createPageRecord(
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

  return {
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
}
