import type { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { parsePageInteractions, type PageInteractions } from "../interactions";

/** Caches per-page interaction metadata rendered from typst.ts semantic output. */
export class InteractionStore {
  private readonly cacheKeys = new Map<number, string>();
  private readonly interactions = new Map<number, PageInteractions>();

  clear() {
    this.cacheKeys.clear();
    this.interactions.clear();
  }

  deletePage(pageIndex: number) {
    this.cacheKeys.delete(pageIndex);
    this.interactions.delete(pageIndex);
  }

  invalidateIfCacheKeyChanged(pageIndex: number, cacheKey: string) {
    if (this.cacheKeys.get(pageIndex) === cacheKey) {
      return undefined;
    }

    const hadInteractions = this.cacheKeys.has(pageIndex) || this.interactions.has(pageIndex);
    this.deletePage(pageIndex);
    return hadInteractions;
  }

  async renderPage(
    session: RenderSession,
    pageIndex: number,
    pixelPerPt: number,
    cacheKey?: string,
  ) {
    const cached = this.interactions.get(pageIndex);
    if (cacheKey && cached && this.cacheKeys.get(pageIndex) === cacheKey) {
      return cached;
    }

    const metadata = await session.renderCanvas({
      pageOffset: pageIndex,
      pixelPerPt,
      dataSelection: {
        body: false,
        semantics: true,
      },
    } as any);
    const interactions = parsePageInteractions(pageIndex, metadata.htmlSemantics?.[0] || "");
    if (cacheKey) {
      this.cacheKeys.set(pageIndex, cacheKey);
    }
    this.interactions.set(pageIndex, interactions);
    return interactions;
  }
}
