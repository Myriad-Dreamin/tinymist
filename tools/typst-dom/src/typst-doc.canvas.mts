import { PreviewMode } from "./typst-doc.mjs";
import { TypstCancellationToken } from "./typst-cancel.mjs";
// import { patchOutlineEntry } from "./typst-outline.mjs";
import { TypstPatchAttrs } from "./typst-patch.mjs";
import type { GConstructor, TypstDocumentContext } from "./typst-doc.mjs";
import type { TypstOutlineDocument } from "./typst-outline.mjs";

export interface CanvasPage {
  tag: "canvas";
  index: number;
  width: number;
  height: number;
  container: HTMLElement;
  elem: HTMLElement;

  // extra properties for patching
  inserter?: (t: CanvasPage) => void;
  stub?: HTMLElement;
}

export interface CreateCanvasOptions {
  defaultInserter?: (page: CanvasPage) => void;
}

export interface UpdateCanvasOptions {
  cancel?: TypstCancellationToken;
  lazy?: boolean;
}

export interface TypstCanvasDocument {
  feat$canvas: boolean;

  createCanvas(pages: CanvasPage[], opts?: CreateCanvasOptions): void;
  updateCanvas(pages: CanvasPage[], opts?: UpdateCanvasOptions): Promise<void>;
}

export function provideCanvasDoc<
  TBase extends GConstructor<
    TypstDocumentContext & Partial<TypstOutlineDocument>
  >,
>(Base: TBase): TBase & GConstructor<TypstCanvasDocument> {
  return class CanvasDocument extends Base {
    feat$canvas = true;

    constructor(...args: any[]) {
      super(...args);
      this.registerMode("canvas");
    }

    private shouldMixinOutline(): this is TypstOutlineDocument {
      return this.isMixinOutline;
    }

    createCanvas(pages: CanvasPage[], opts?: CreateCanvasOptions): void {
      // get dom state from cache, so we are free from layout reflowing
      const docDiv = this.hookedElem.firstElementChild! as HTMLDivElement;
      const rescale = this.rescaleOne(docDiv);

      let isFirst = true;
      for (const pageInfo of pages) {
        if (!pageInfo.elem) {
          pageInfo.elem = document.createElement("div");
          pageInfo.elem.setAttribute("class", "typst-page-canvas");
          pageInfo.elem.style.transformOrigin = "0 0";
          pageInfo.elem.setAttribute(
            "data-page-number",
            pageInfo.index.toString()
          );

          const canvas = document.createElement("canvas");
          pageInfo.elem.appendChild(canvas);

          pageInfo.container = document.createElement("div");
          // todo: reuse by key
          pageInfo.container.setAttribute(
            TypstPatchAttrs.Tid,
            `canvas:` + pageInfo.index
          );
          pageInfo.container.setAttribute("class", "typst-page canvas-mode");
          pageInfo.container.setAttribute(
            "data-page-number",
            pageInfo.index.toString()
          );
          pageInfo.container.appendChild(pageInfo.elem);

          // do scaling early
          this.prepareCanvas(pageInfo, canvas);
          rescale(pageInfo.container, this.isContentPreview || isFirst);

          if (this.isContentPreview) {
            const pageNumberIndicator = document.createElement("div");
            pageNumberIndicator.setAttribute(
              "class",
              "typst-preview-canvas-page-number"
            );
            pageNumberIndicator.textContent = `${pageInfo.index + 1}`;
            pageInfo.container.appendChild(pageNumberIndicator);

            pageInfo.container.style.cursor = "pointer";
            pageInfo.container.style.pointerEvents = "visible";
            pageInfo.container.style.overflow = "hidden";
            pageInfo.container.addEventListener("click", () => {
              // console.log('click', pageInfo.index);
              window.typstWebsocket.send(`outline-sync,${pageInfo.index + 1}`);
            });
          }
        }

        if (!pageInfo.container.parentElement) {
          if (pageInfo.inserter) {
            pageInfo.inserter(pageInfo);
          } else if (opts?.defaultInserter) {
            opts.defaultInserter(pageInfo);
          } else {
            throw new Error("pageInfo.inserter is not defined");
          }
        }

        isFirst = false;
      }
    }

    prepareCanvas(pageInfo: CanvasPage, canvas: HTMLCanvasElement) {
      const pw = pageInfo.width;
      const ph = pageInfo.height;
      const pws = pageInfo.width.toFixed(3);
      const phs = pageInfo.height.toFixed(3);

      let cached = true;

      if (pageInfo.elem.getAttribute("data-page-width") !== pws) {
        pageInfo.elem.setAttribute("data-page-width", pws);
        cached = false;
        canvas.width = pw * this.pixelPerPt;
      }

      if (pageInfo.elem.getAttribute("data-page-height") !== phs) {
        pageInfo.elem.setAttribute("data-page-height", phs);
        cached = false;
        canvas.height = ph * this.pixelPerPt;
      }

      return cached;
    }

    async updateCanvas(
      pages: CanvasPage[],
      opts?: UpdateCanvasOptions
    ): Promise<void> {
      const tok = opts?.cancel || undefined;
      const perf = performance.now();
      console.log("updateCanvas start");
      // todo: priority in window
      // await Promise.all(pagesInfo.map(async (pageInfo) => {
      this.kModule.backgroundColor = "#ffffff";
      this.kModule.pixelPerPt = this.pixelPerPt;
      const waitABit = async () => {
        return new Promise((resolve) => {
          if (opts?.lazy) {
            requestIdleCallback(() => resolve(undefined), { timeout: 100 });
          } else {
            resolve(undefined);
          }
        });
      };
      for (const pageInfo of pages) {
        if (tok?.isCancelRequested()) {
          await tok.consume();
          console.log("updateCanvas cancelled", performance.now() - perf);
          return;
        }

        const canvas = pageInfo.elem.firstElementChild as HTMLCanvasElement;
        // const tt1 = performance.now();

        const pws = pageInfo.width.toFixed(3);
        const phs = pageInfo.height.toFixed(3);

        let cached = this.prepareCanvas(pageInfo, canvas);

        const cacheKey =
          pageInfo.elem.getAttribute("data-cache-key") || undefined;
        const result = await this.kModule.renderCanvas({
          canvas: canvas.getContext("2d")!,
          pageOffset: pageInfo.index,
          cacheKey: cached ? cacheKey : undefined,
          dataSelection: {
            body: true,
          },
        });
        if (cacheKey !== result.cacheKey) {
          console.log("updateCanvas one miss", cacheKey, result.cacheKey);
          // console.log('renderCanvas', pageInfo.index, performance.now() - tt1, result);
          // todo: cache key changed
          // canvas.width = pageInfo.width * this.pixelPerPt;
          // canvas.height = pageInfo.height * this.pixelPerPt;
          pageInfo.elem.setAttribute("data-page-width", pws);
          pageInfo.elem.setAttribute("data-page-height", phs);
          canvas.setAttribute("data-cache-key", result.cacheKey);
          pageInfo.elem.setAttribute("data-cache-key", result.cacheKey);
        }

        await waitABit();
      }

      console.log("updateCanvas done", performance.now() - perf);
      await tok?.consume();
    }

    rescaleOne(docDiv: HTMLDivElement) {
      // get dom state from cache, so we are free from layout reflowing
      // Note: one should retrieve dom state before rescale
      const { width: cwRaw, height: ch } = this.cachedDOMState;
      const cw = this.isContentPreview ? cwRaw - 10 : cwRaw;

      return (canvasContainer: HTMLElement, noSpacingFromTop: boolean) => {
        // console.log(ch);
        if (noSpacingFromTop) {
          canvasContainer.style.marginTop = `0px`;
        } else {
          canvasContainer.style.marginTop = `${
            this.isContentPreview ? 6 : 5
          }px`;
        }
        let elem = canvasContainer.firstElementChild as HTMLDivElement;

        const canvasWidth = Number.parseFloat(
          elem.getAttribute("data-page-width")!
        );
        const canvasHeight = Number.parseFloat(
          elem.getAttribute("data-page-height")!
        );

        this.currentRealScale =
          this.previewMode === PreviewMode.Slide
            ? Math.min(cw / canvasWidth, ch / canvasHeight)
            : cw / canvasWidth;
        const scale = this.currentRealScale * this.currentScaleRatio;

        // apply scale
        const appliedScale = (scale / this.pixelPerPt).toString();

        // set data applied width and height to memoize change
        if (elem.getAttribute("data-applied-scale") !== appliedScale) {
          elem.setAttribute("data-applied-scale", appliedScale);
          // apply translate
          const scaledWidth = Math.ceil(canvasWidth * scale);
          const scaledHeight = Math.ceil(canvasHeight * scale);

          elem.style.width = `${scaledWidth}px`;
          elem.style.height = `${scaledHeight}px`;
          elem.style.transform = `scale(${appliedScale})`;

          if (this.previewMode === PreviewMode.Slide) {
            const widthAdjust = Math.max((cw - scaledWidth) / 2, 0);
            const heightAdjust = Math.max((ch - scaledHeight) / 2, 0);
            docDiv.style.transform = `translate(${widthAdjust}px, ${heightAdjust}px)`;
          }
        }
      };
    }

    rescale$canvas() {
      // get dom state from cache, so we are free from layout reflowing
      const docDiv = this.hookedElem.firstElementChild! as HTMLDivElement;
      if (!docDiv) {
        return;
      }

      let isFirst = true;
      const rescale = this.rescaleOne(docDiv);

      if (this.isContentPreview) {
        const rescaleChildren = (elem: HTMLElement) => {
          for (const ch of elem.children) {
            let canvasContainer = ch as HTMLElement;
            if (canvasContainer.classList.contains("typst-page")) {
              rescale(canvasContainer, true);
            }
            if (canvasContainer.classList.contains("typst-outline")) {
              rescaleChildren(canvasContainer);
            }
          }
        };

        rescaleChildren(docDiv);
      } else {
        for (const ch of docDiv.children) {
          let canvasContainer = ch as HTMLDivElement;
          if (!canvasContainer.classList.contains("typst-page")) {
            continue;
          }
          rescale(canvasContainer, isFirst);
          isFirst = false;
        }
      }
    }

    postRender$canvas() {
      this.r.rescale();
    }

    async rerender$canvas() {
      // console.log('toggleCanvasViewportChange!!!!!!', this.id, this.isRendering);
      const pages: CanvasPage[] = this.kModule
        .retrievePagesInfo()
        .map((x, index) => {
          return {
            tag: "canvas",
            index,
            width: x.width,
            height: x.height,
            container: undefined as any as HTMLDivElement,
            elem: undefined as any as HTMLDivElement,
          };
        });

      if (!this.hookedElem.firstElementChild) {
        this.hookedElem.innerHTML = `<div class="typst-doc" data-render-mode="canvas"></div>`;
      }
      const docDiv = this.hookedElem.firstElementChild! as HTMLDivElement;

      if (this.shouldMixinOutline() && this.outline) {
        console.log("render with outline", this.outline);
        this.patchOutlineEntry(docDiv as any, pages, this.outline.items);

        const checkChildren = (elem: HTMLElement) => {
          for (const ch of elem.children) {
            let canvasContainer = ch as HTMLElement;
            if (canvasContainer.classList.contains("typst-outline")) {
              checkChildren(canvasContainer);
            }
            if (canvasContainer.classList.contains("typst-page")) {
              const pageNumber = Number.parseInt(
                ch.getAttribute("data-page-number")!
              );
              if (pageNumber >= pages.length) {
                // todo: cache key can shifted
                elem.removeChild(ch);
              } else {
                pages[pageNumber].container = ch as HTMLDivElement;
                pages[pageNumber].elem = ch.firstElementChild as HTMLDivElement;
              }
            }
          }
        };

        checkChildren(docDiv);
      } else {
        for (const ch of docDiv.children) {
          if (!ch.classList.contains("typst-page")) {
            continue;
          }
          const pageNumber = Number.parseInt(
            ch.getAttribute("data-page-number")!
          );
          if (pageNumber >= pages.length) {
            // todo: cache key shifted
            docDiv.removeChild(ch);
            continue;
          }
          pages[pageNumber].container = ch as HTMLDivElement;
          pages[pageNumber].elem = ch.firstElementChild as HTMLDivElement;
        }
      }

      this.createCanvas(pages, {
        defaultInserter: (page) => {
          if (page.index === 0) {
            docDiv.prepend(page.container);
          } else {
            pages[page.index - 1].container.after(page.container);
          }
        },
      });

      const t2 = performance.now();

      if (docDiv.getAttribute("data-rendering") === "true") {
        throw new Error("rendering in progress, possibly a race condition");
      }
      docDiv.setAttribute("data-rendering", "true");
      await this.updateCanvas(pages);
      docDiv.removeAttribute("data-rendering");
      // }));

      const t3 = performance.now();

      return [t2, t3];
    }
  };
}
