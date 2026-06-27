import type { CanvasEntry, CanvasLayer, PageRenderSpec } from "./types";
import { commitPixelRect } from "./render-utils";

/** Owns worker-side canvas entries, render cache keys, scratch buffers, and LRU eviction. */
export class CanvasStore {
  private readonly entries = new Map<number, CanvasEntry>();
  private readonly lru = new Map<number, true>();
  private readonly renderCacheKeys = new Map<number, string>();
  private readonly latestCacheKeys = new Map<number, string>();

  constructor(private readonly maxEntries: number) {}

  get(pageIndex: number) {
    return this.entries.get(pageIndex);
  }

  indices() {
    return this.entries.keys();
  }

  setCanvas(pageIndex: number, canvas: OffscreenCanvas, widthPx: number, heightPx: number) {
    this.entries.set(pageIndex, {
      canvas,
      widthPx,
      heightPx,
    });
    this.renderCacheKeys.delete(pageIndex);
  }

  deletePage(pageIndex: number) {
    this.entries.delete(pageIndex);
    this.lru.delete(pageIndex);
    this.renderCacheKeys.delete(pageIndex);
    this.latestCacheKeys.delete(pageIndex);
  }

  clear() {
    this.entries.clear();
    this.lru.clear();
    this.renderCacheKeys.clear();
    this.latestCacheKeys.clear();
  }

  getRenderCacheKey(pageIndex: number) {
    return this.renderCacheKeys.get(pageIndex);
  }

  setRenderCacheKey(pageIndex: number, cacheKey: string) {
    this.renderCacheKeys.set(pageIndex, cacheKey);
  }

  deleteRenderCacheKey(pageIndex: number) {
    this.renderCacheKeys.delete(pageIndex);
  }

  getLatestCacheKey(pageIndex: number) {
    return this.latestCacheKeys.get(pageIndex);
  }

  setLatestCacheKey(pageIndex: number, cacheKey: string) {
    this.latestCacheKeys.set(pageIndex, cacheKey);
  }

  touch(pageIndex: number) {
    this.lru.delete(pageIndex);
    this.lru.set(pageIndex, true);
  }

  resizeEntry(entry: CanvasEntry, pageIndex: number, widthPx: number, heightPx: number) {
    let resized = false;
    if (entry.canvas.width !== widthPx) {
      entry.canvas.width = widthPx;
      entry.hasContent = false;
      this.renderCacheKeys.delete(pageIndex);
      resized = true;
    }
    if (entry.canvas.height !== heightPx) {
      entry.canvas.height = heightPx;
      entry.hasContent = false;
      this.renderCacheKeys.delete(pageIndex);
      resized = true;
    }
    entry.widthPx = widthPx;
    entry.heightPx = heightPx;
    return resized;
  }

  ensureContext(entry: CanvasEntry, pageIndex: number) {
    const context =
      entry.context ||
      entry.canvas.getContext("2d", {
        alpha: false,
        desynchronized: true,
      });
    if (!context) {
      throw new Error(`cannot create 2d context for page ${pageIndex}`);
    }
    entry.context = context;
    return context;
  }

  ensureScratchContext(entry: CanvasEntry, pageIndex: number, layer: CanvasLayer) {
    if (!entry.scratch) {
      entry.scratch = new OffscreenCanvas(entry.widthPx, entry.heightPx);
    }
    if (entry.scratch.width !== entry.widthPx) {
      entry.scratch.width = entry.widthPx;
    }
    if (entry.scratch.height !== entry.heightPx) {
      entry.scratch.height = entry.heightPx;
    }

    const context =
      entry.scratchContext ||
      entry.scratch.getContext("2d", {
        alpha: false,
        desynchronized: true,
      });
    if (!context) {
      throw new Error(`cannot create ${layer} scratch context for page ${pageIndex}`);
    }
    entry.scratchContext = context;
    return context;
  }

  releaseScratch(entry: CanvasEntry) {
    if (!entry.scratch) {
      return;
    }
    if (entry.scratch.width !== 1) {
      entry.scratch.width = 1;
    }
    if (entry.scratch.height !== 1) {
      entry.scratch.height = 1;
    }
    entry.scratchContext = undefined;
  }

  initializeTargetCanvas(target: OffscreenCanvasRenderingContext2D, entry: CanvasEntry) {
    target.setTransform(1, 0, 0, 1, 0, 0);
    target.fillStyle = "#ffffff";
    target.fillRect(0, 0, entry.widthPx, entry.heightPx);
  }

  commitScratchToTarget(
    entry: CanvasEntry,
    target: OffscreenCanvasRenderingContext2D,
    scratch: OffscreenCanvasRenderingContext2D,
    page: PageRenderSpec,
    pixelPerPt: number,
  ) {
    target.setTransform(1, 0, 0, 1, 0, 0);
    const rect = commitPixelRect(page, pixelPerPt, entry.widthPx, entry.heightPx);
    if (!rect) {
      return;
    }
    target.clearRect(rect.x, rect.y, rect.width, rect.height);
    target.drawImage(
      scratch.canvas,
      rect.x,
      rect.y,
      rect.width,
      rect.height,
      rect.x,
      rect.y,
      rect.width,
      rect.height,
    );
  }

  enforceLimit(protectedPages: Set<number>) {
    if (this.lru.size <= this.maxEntries) {
      return [];
    }

    const evicted: number[] = [];
    while (this.lru.size > this.maxEntries) {
      const candidate = [...this.lru.keys()].find((pageIndex) => !protectedPages.has(pageIndex));
      if (candidate === undefined) {
        break;
      }

      const entry = this.entries.get(candidate);
      if (entry) {
        this.evictEntry(entry);
      }
      this.lru.delete(candidate);
      this.renderCacheKeys.delete(candidate);
      this.latestCacheKeys.delete(candidate);
      evicted.push(candidate);
    }
    return evicted;
  }

  private evictEntry(entry: CanvasEntry) {
    entry.canvas.width = 1;
    entry.canvas.height = 1;
    entry.widthPx = 1;
    entry.heightPx = 1;
    entry.hasContent = false;
    entry.quality = undefined;
    entry.renderedGeneration = undefined;
    entry.renderedPixelPerPt = undefined;
    entry.renderedWindowKey = undefined;
    entry.context = undefined;
    this.releaseScratch(entry);
  }
}
