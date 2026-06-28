import type { PageRect } from "../interactions";
import type { CanvasEntry } from "./types";
import { clamp } from "./render-utils";

export interface TextHitResult {
  hit: boolean;
  rect?: PageRect;
}

export function hitRenderedTextBox(
  entry: CanvasEntry | undefined,
  pixelPerPt: number | undefined,
  x: number,
  y: number,
  rect?: PageRect,
): TextHitResult {
  const context = entry?.context;
  if (!entry || !context || !pixelPerPt) {
    return { hit: false };
  }

  const px = Math.floor(x * pixelPerPt);
  const py = Math.floor(y * pixelPerPt);
  if (rect && usesTextRunBounds(rect)) {
    const textRect = textRunInkBounds(context, entry, pixelPerPt, rect, py);
    return textRect ? { hit: true, rect: textRect } : { hit: false };
  }

  const start = nearestInkPixel(context, entry, px, py, pixelPerPt, rect);
  if (!start) {
    return { hit: false };
  }

  return {
    hit: true,
    rect: connectedInkBounds(context, entry, start.x, start.y, pixelPerPt),
  };
}

export function resolveRenderedTextRect(
  entry: CanvasEntry | undefined,
  pixelPerPt: number | undefined,
  rect: PageRect,
) {
  const context = entry?.context;
  if (!entry || !context || !pixelPerPt) {
    return undefined;
  }
  return textRunInkBounds(context, entry, pixelPerPt, rect);
}

function textRunInkBounds(
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
  const rowClusters = textInkRowClusters(pixels, width, height, top);
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

function textInkRowClusters(pixels: Uint8ClampedArray, width: number, height: number, top: number) {
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

function nearestInkPixel(
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
    const pad = Math.trunc(clamp(Math.max(rectRight - rectLeft, rectBottom - rectTop) * 2, 16, 96));
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

function connectedInkBounds(
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
