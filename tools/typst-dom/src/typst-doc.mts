import type { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";

export interface ContainerDOMState {
  /// cached `hookedElem.offsetWidth` or `hookedElem.innerWidth`
  width: number;
  /// cached `hookedElem.offsetHeight` or `hookedElem.innerHeight`
  height: number;
  /// cached `hookedElem.getBoundingClientRect()`
  /// We only use `left` and `top` here.
  boundingRect: {
    left: number;
    top: number;
  };
}

export type RenderMode = "svg" | "canvas";

export enum PreviewMode {
  Doc,
  Slide,
}

export interface Options {
  hookedElem: HTMLElement;
  kModule: RenderSession;
  renderMode?: RenderMode;
  previewMode?: PreviewMode;
  isContentPreview?: boolean;
  sourceMapping?: boolean;
  retrieveDOMState?: () => ContainerDOMState;
}

export type GConstructor<T = {}> = new (...args: any[]) => T;

interface TypstDocumentFacade {
  rescale(): void;
  rerender(): Promise<void>;
  postRender(): void;
}

export class TypstDocumentContext<O = any> {
  public hookedElem: HTMLElement;
  public kModule: RenderSession;
  public opts: O;
  modes: [string, TypstDocumentFacade][] = [];

  /// Configuration fields

  /// enable partial rendering
  partialRendering: boolean = true;
  /// underlying renderer
  renderMode: RenderMode = "svg";
  r: TypstDocumentFacade = undefined!;
  /// preview mode
  previewMode: PreviewMode = PreviewMode.Doc;
  /// whether this is a content preview
  isContentPreview: boolean = false;
  /// whether this content preview will mix outline titles
  isMixinOutline: boolean = false;
  /// background color
  backgroundColor: string = "black";
  /// default page color (empty string means transparent)
  pageColor: string = "white";
  /// pixel per pt
  pixelPerPt: number = 3;
  /// customized way to retrieving dom state
  retrieveDOMState: () => ContainerDOMState;

  /// State fields

  /// whether svg is updating (in triggerSvgUpdate)
  isRendering: boolean = false;
  /// whether kModule is initialized
  moduleInitialized: boolean = false;
  /// patch queue for updating data.
  patchQueue: [string, string][] = [];
  /// resources to dispose
  disposeList: (() => void)[] = [];

  /// There are two scales in this class: The real scale is to adjust the size
  /// of `hookedElem` to fit the svg. The virtual scale (scale ratio) is to let
  /// user zoom in/out the svg. For example:
  /// + the default value of virtual scale is 1, which means the svg is totally
  ///   fit in `hookedElem`.
  /// + if user set virtual scale to 0.5, then the svg will be zoomed out to fit
  ///   in half width of `hookedElem`. "real" current scale of `hookedElem`
  currentRealScale: number = 1;
  /// "virtual" current scale of `hookedElem`
  currentScaleRatio: number = 1;
  /// timeout for delayed viewport change
  vpTimeout: any = undefined;
  /// sampled by last render time.
  sampledRenderTime: number = 0;
  /// page to partial render
  partialRenderPage: number = 0;
  /// outline data
  outline: any = undefined;
  /// cursor position in form of [page, x, y]
  cursorPosition?: [number, number, number] = undefined;
  // id: number = rnd++;

  /// Cache fields

  /// cached state of container, default to retrieve state from `this.hookedElem`
  cachedDOMState: ContainerDOMState = {
    width: 0,
    height: 0,
    boundingRect: {
      left: 0,
      top: 0,
    },
  };

  constructor(opts: Options & O) {
    this.hookedElem = opts.hookedElem;
    this.kModule = opts.kModule;
    this.opts = opts || {};

    /// Apply configuration
    {
      const { renderMode, previewMode, isContentPreview, retrieveDOMState } =
        opts || {};
      this.partialRendering = false;
      this.renderMode = renderMode ?? this.renderMode;
      this.previewMode = previewMode ?? this.previewMode;
      this.isContentPreview = isContentPreview || false;
      this.retrieveDOMState =
        retrieveDOMState ??
        (() => {
          return {
            width: this.hookedElem.offsetWidth,
            height: this.hookedElem.offsetHeight,
            boundingRect: this.hookedElem.getBoundingClientRect(),
          };
        });
      this.backgroundColor = getComputedStyle(
        document.documentElement
      ).getPropertyValue("--typst-preview-background-color");
    }

    // if init scale == 1
    // hide scrollbar if scale == 1

    this.hookedElem.classList.add("hide-scrollbar-x");
    this.hookedElem.parentElement?.classList.add("hide-scrollbar-x");
    if (this.previewMode === PreviewMode.Slide) {
      this.hookedElem.classList.add("hide-scrollbar-y");
      this.hookedElem.parentElement?.classList.add("hide-scrollbar-y");
    }

    this.installRescaleHandler();
  }

  reset() {
    this.kModule.reset();
    this.moduleInitialized = false;
  }

  dispose() {
    const disposeList = this.disposeList;
    this.disposeList = [];
    disposeList.forEach((x) => x());
  }

  static derive(ctx: any, mode: string) {
    return ["rescale", "rerender", "postRender"].reduce(
      (acc: any, x: string) => {
        let index = x + "$" + mode;
        acc[x] = ctx[index].bind(ctx);
        console.assert(acc[x] !== undefined, `${x}$${mode} is undefined`);
        return acc;
      },
      {} as TypstDocumentFacade
    );
  }

  registerMode(mode: any) {
    const facade = TypstDocumentContext.derive(this, mode);
    this.modes.push([mode, facade]);
    if (mode === this.renderMode) {
      this.r = facade;
    }
  }

  private installRescaleHandler() {

    // Ctrl+scroll and Ctrl+=/- rescaling
    // will disable auto resizing
    // fixed factors, same as pdf.js
    const factors = [
      0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9,
      2.1, 2.4, 2.7, 3, 3.3, 3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
    ];
    const doRescale = (scrollDirection: number, pageX: number | undefined, pageY: number | undefined) => {
      const prevScaleRatio = this.currentScaleRatio;
      // Get wheel scroll direction and calculate new scale
      if (scrollDirection === -1) {
        // enlarge
        if (this.currentScaleRatio >= factors.at(-1)!) {
          // already large than max factor
          return;
        } else {
          this.currentScaleRatio = factors
            .filter((x) => x > this.currentScaleRatio)
            .at(0)!;
        }
      } else if (scrollDirection === 1) {
        // reduce
        if (this.currentScaleRatio <= factors.at(0)!) {
          return;
        } else {
          this.currentScaleRatio = factors
            .filter((x) => x < this.currentScaleRatio)
            .at(-1)!;
        }
      } else {
        // no y-axis scroll
        return;
      }
      const scrollFactor = this.currentScaleRatio / prevScaleRatio;

      // hide scrollbar if scale == 1
      if (Math.abs(this.currentScaleRatio - 1) < 1e-5) {
        this.hookedElem.classList.add("hide-scrollbar-x");
        this.hookedElem.parentElement?.classList.add("hide-scrollbar-x");
        if (this.previewMode === PreviewMode.Slide) {
          this.hookedElem.classList.add("hide-scrollbar-y");
          this.hookedElem.parentElement?.classList.add("hide-scrollbar-y");
        }
      } else {
        this.hookedElem.classList.remove("hide-scrollbar-x");
        this.hookedElem.parentElement?.classList.remove("hide-scrollbar-x");
        if (this.previewMode === PreviewMode.Slide) {
          this.hookedElem.classList.remove("hide-scrollbar-y");
          this.hookedElem.parentElement?.classList.remove("hide-scrollbar-y");
        }
      }

      // reserve space to scroll down
      const svg = this.hookedElem.firstElementChild! as SVGElement;
      if (svg) {
        const scaleRatio = this.getSvgScaleRatio();

        const dataHeight = Number.parseFloat(
          svg.getAttribute("data-height")!
        );
        const scaledHeight = Math.ceil(dataHeight * scaleRatio);

        // we increase the height by 2 times.
        // The `2` is only a magic number that is large enough.
        this.hookedElem.style.height = `${scaledHeight * 2}px`;
      }

      // make sure the cursor is still on the same position
      if(pageX !== undefined && pageY !== undefined) {
        const scrollX = pageX * (scrollFactor - 1);
        const scrollY = pageY * (scrollFactor - 1);
        window.scrollBy(scrollX, scrollY);
      }
      // toggle scale change event
      this.addViewportChange();
    };

    // Ctrl+= or Ctrl+- rescaling
    const isMac = navigator.platform.toUpperCase().indexOf('MAC') !== -1;
    const keydownEventHandler = (event: KeyboardEvent) => {
      if((!isMac && event.ctrlKey) || (isMac && event.metaKey)) {
        if(event.key === "=") {
          event.preventDefault();
          doRescale(-1, undefined, undefined);
          return false;
        } else if(event.key === "-") {
          event.preventDefault();
          doRescale(+1, undefined, undefined);
          return false;
        }
      }
    };

    // Ctrl+scroll rescaling
    const deltaDistanceThreshold = 20;
    const pixelPerLine = 20;
    let deltaDistance = 0;
    const wheelEventHandler = (event: WheelEvent) => {
      if (event.ctrlKey) {
        event.preventDefault();

        // retrieve dom state before any operation
        this.cachedDOMState = this.retrieveDOMState();

        if (window.onresize !== null) {
          // is auto resizing
          window.onresize = null;
        }
        // accumulate delta distance
        const pixels = event.deltaMode === 0 ? event.deltaY : event.deltaY * pixelPerLine;
        deltaDistance += pixels;
        if (Math.abs(deltaDistance) < deltaDistanceThreshold) {
          return;
        }
        const scrollDirection = deltaDistance > 0 ? 1 : -1;
        deltaDistance = 0;
        doRescale(scrollDirection, event.pageX, event.pageY);
        return false;
      }
    };

    const vscodeAPI = typeof acquireVsCodeApi !== "undefined";
    if (vscodeAPI) {
      window.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
      this.disposeList.push(() => {
        window.removeEventListener("wheel", wheelEventHandler);
      });
    } else {
      document.body.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
      document.body.addEventListener("keydown", keydownEventHandler);
      this.disposeList.push(() => {
        document.body.removeEventListener("wheel", wheelEventHandler);
        document.body.removeEventListener("keydown", keydownEventHandler);
      });
    }
  }

  /// Get current scale from html to svg
  // Note: one should retrieve dom state before rescale
  getSvgScaleRatio() {
    const svg = this.hookedElem.firstElementChild as SVGElement;
    if (!svg) {
      return 0;
    }

    const container = this.cachedDOMState;

    const svgWidth = Number.parseFloat(
      svg.getAttribute("data-width") || svg.getAttribute("width") || "1"
    );
    const svgHeight = Number.parseFloat(
      svg.getAttribute("data-height") || svg.getAttribute("height") || "1"
    );
    this.currentRealScale =
      this.previewMode === PreviewMode.Slide
        ? Math.min(container.width / svgWidth, container.height / svgHeight)
        : container.width / svgWidth;

    return this.currentRealScale * this.currentScaleRatio;
  }

  private processQueue(svgUpdateEvent: [string, string]): boolean {
    const eventName = svgUpdateEvent[0];
    switch (eventName) {
      case "new":
      case "diff-v1": {
        if (eventName === "new") {
          this.reset();
        }
        this.kModule.manipulateData({
          action: "merge",
          data: svgUpdateEvent[1] as unknown as Uint8Array,
        });

        this.moduleInitialized = true;
        return true;
      }
      case "viewport-change": {
        if (!this.moduleInitialized) {
          console.log("viewport-change before initialization");
          return false;
        }
        return true;
      }
      default:
        console.log("svgUpdateEvent", svgUpdateEvent);
        return false;
    }
  }

  private triggerUpdate() {
    if (this.isRendering) {
      return;
    }

    this.isRendering = true;
    const doUpdate = async () => {
      this.cachedDOMState = this.retrieveDOMState();

      if (this.patchQueue.length === 0) {
        this.isRendering = false;
        this.postprocessChanges();
        return;
      }

      try {
        let t0 = performance.now();

        let needRerender = false;
        // console.log('patchQueue', JSON.stringify(this.patchQueue.map(x => x[0])));
        while (this.patchQueue.length > 0) {
          needRerender = this.processQueue(this.patchQueue.shift()!) || needRerender;
        }

        // todo: trigger viewport change once
        let t1 = performance.now();
        if (needRerender) {
          this.r.rescale();
          await this.r.rerender();
          this.r.rescale();
        }
        let t2 = performance.now();

        /// perf event
        const d = (e: string, x: number, y: number) =>
          `${e} ${(y - x).toFixed(2)} ms`;
        this.sampledRenderTime = t2 - t0;
        console.log(
          [d("parse", t0, t1), d("rerender", t1, t2), d("total", t0, t2)].join(", ")
        );

        requestAnimationFrame(doUpdate);
      } catch (e) {
        console.error(e);
        this.isRendering = false;
        this.postprocessChanges();
      }
    };
    requestAnimationFrame(doUpdate);
  }

  private postprocessChanges() {
    // case RenderMode.Svg: {
    // const docRoot = this.hookedElem.firstElementChild as SVGElement;
    // if (docRoot) {
    //   window.initTypstSvg(docRoot);
    //   this.rescale();
    // }

    this.r.postRender();

    // todo: abstract this
    if (this.previewMode === PreviewMode.Slide) {
      document.querySelectorAll(".typst-page-number-indicator").forEach((x) => {
        x.textContent = `${this.kModule.retrievePagesInfo().length}`;
      });
    }
  }

  addChangement(change: [string, string]) {
    if (change[0] === "new") {
      this.patchQueue.splice(0, this.patchQueue.length);
    }

    const pushChange = () => {
      this.vpTimeout = undefined;
      this.patchQueue.push(change);
      this.triggerUpdate();
    };

    if (this.vpTimeout !== undefined) {
      clearTimeout(this.vpTimeout);
    }

    if (change[0] === "viewport-change" && this.isRendering) {
      // delay viewport change a bit
      this.vpTimeout = setTimeout(pushChange, this.sampledRenderTime || 100);
    } else {
      pushChange();
    }
  }

  addViewportChange() {
    this.addChangement(["viewport-change", ""]);
  }
}

export interface TypstDocument<T> {
  impl: T;
  kModule: RenderSession;
  dispose(): void;
  reset(): void;
  addChangement(change: [string, string]): void;
  addViewportChange(): void;
  setPageColor(color: string): void;
  setPartialRendering(partialRendering: boolean): void;
  setCursor(page: number, x: number, y: number): void;
  setPartialPageNumber(page: number): boolean;
  getPartialPageNumber(): number;
  setOutineData(outline: any): void;
}

export function provideDoc<T extends TypstDocumentContext>(
  Base: GConstructor<T>
): new (options: Options) => TypstDocument<T> {
  return class TypstDocument {
    public impl: T;
    public kModule: RenderSession;

    constructor(options: Options) {
      if (options.isContentPreview) {
        options.renderMode = "canvas";
      }

      this.kModule = options.kModule;
      this.impl = new Base(options);
      if (!this.impl.r) {
        throw new Error(`mode is not supported, ${options?.renderMode}`);
      }

      if (options.isContentPreview) {
        // content preview has very bad performance without partial rendering
        this.impl.partialRendering = true;
        this.impl.pixelPerPt = 1;
        this.impl.isMixinOutline = true;
      }
    }

    dispose() {
      this.impl.dispose();
    }

    reset() {
      this.impl.reset();
    }

    addChangement(change: [string, string]) {
      this.impl.addChangement(change);
    }

    addViewportChange() {
      this.impl.addViewportChange();
    }

    setPageColor(color: string) {
      this.impl.pageColor = color;
      this.addViewportChange();
    }

    setPartialRendering(partialRendering: boolean) {
      this.impl.partialRendering = partialRendering;
    }

    setCursor(page: number, x: number, y: number) {
      this.impl.cursorPosition = [page, x, y];
    }

    setPartialPageNumber(page: number): boolean {
      if (page <= 0 || page > this.kModule.retrievePagesInfo().length) {
        return false;
      }
      this.impl.partialRenderPage = page - 1;
      this.addViewportChange();
      return true;
    }

    getPartialPageNumber(): number {
      return this.impl.partialRenderPage + 1;
    }

    setOutineData(outline: any) {
      this.impl.outline = outline;
      this.addViewportChange();
    }
  };
}

export function composeDoc<TBase extends GConstructor, F1>(
  Base: TBase,
  f1: (base: TBase) => F1
): TBase & F1;
export function composeDoc<TBase extends GConstructor, F1, F2>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2
): TBase & F1 & F2;
export function composeDoc<TBase extends GConstructor, F1, F2, F3>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3
): TBase & F1 & F2 & F3;
export function composeDoc<TBase extends GConstructor, F1, F2, F3, F4>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4
): TBase & F1 & F2 & F3 & F4;
export function composeDoc<TBase extends GConstructor, F1, F2, F3, F4, F5>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4,
  f5: (base: F4) => F5
): TBase & F1 & F2 & F3 & F4 & F5;
export function composeDoc<TBase extends GConstructor, F1, F2, F3, F4, F5, F6>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4,
  f5: (base: F4) => F5,
  f6: (base: F5) => F6
): TBase & F1 & F2 & F3 & F4 & F5 & F6;
export function composeDoc<
  TBase extends GConstructor,
  F1,
  F2,
  F3,
  F4,
  F5,
  F6,
  F7
>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4,
  f5: (base: F4) => F5,
  f6: (base: F5) => F6,
  f7: (base: F6) => F7
): TBase & F1 & F2 & F3 & F4 & F5 & F6 & F7;
export function composeDoc<
  TBase extends GConstructor,
  F1,
  F2,
  F3,
  F4,
  F5,
  F6,
  F7,
  F8
>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4,
  f5: (base: F4) => F5,
  f6: (base: F5) => F6,
  f7: (base: F6) => F7,
  f8: (base: F7) => F8
): TBase & F1 & F2 & F3 & F4 & F5 & F6 & F7 & F8;
export function composeDoc<
  TBase extends GConstructor,
  F1,
  F2,
  F3,
  F4,
  F5,
  F6,
  F7,
  F8,
  F9
>(
  Base: TBase,
  f1: (base: TBase) => F1,
  f2: (base: F1) => F2,
  f3: (base: F2) => F3,
  f4: (base: F3) => F4,
  f5: (base: F4) => F5,
  f6: (base: F5) => F6,
  f7: (base: F6) => F7,
  f8: (base: F7) => F8,
  f9: (base: F8) => F9
): TBase & F1 & F2 & F3 & F4 & F5 & F6 & F7 & F8 & F9;
export function composeDoc<TBase extends GConstructor>(
  Base: TBase,
  ...mixins: any[]
): TBase {
  return mixins.reduce((acc, mixin) => mixin(acc), Base);
}
