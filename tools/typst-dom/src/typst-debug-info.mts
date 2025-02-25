import { triggerRipple } from "./typst-animation.mjs";
import type { GConstructor, TypstDocumentContext } from "./typst-doc.mjs";

const enum SourceMappingType {
  Text = 0,
  Group = 1,
  Image = 2,
  Shape = 3,
  Page = 4,
  CharIndex = 5,
}

export interface ElementPoint {
  kind: number;
  index: number;
  fingerprint: string;
}

// one-of following classes must be present:
// - typst-page
// - typst-group
// - typst-text
// - typst-image
// - typst-shape
const CssClassToType = [
  ["typst-text", SourceMappingType.Text],
  ["typst-group", SourceMappingType.Group],
  ["typst-image", SourceMappingType.Image],
  ["typst-shape", SourceMappingType.Shape],
  ["typst-page", SourceMappingType.Page],
  ["tsel", SourceMappingType.CharIndex],
] as const;

function castToSourceMappingElement(
  elem: Element,
): [SourceMappingType, Element, string] | undefined {
  if (elem.classList.length === 0) {
    return undefined;
  }
  for (const [cls, ty] of CssClassToType) {
    if (elem.classList.contains(cls)) {
      return [ty, elem, ""];
    }
  }
  return undefined;
}

function castToNestSourceMappingElement(
  elem: Element,
): [SourceMappingType, Element, string] | undefined {
  while (elem) {
    const result = castToSourceMappingElement(elem);
    if (result) {
      return result;
    }
    let chs = elem.children;
    if (chs.length !== 1) {
      return undefined;
    }
    elem = chs[0];
  }

  return undefined;
}

function castChildrenToSourceMappingElement(elem: Element): [SourceMappingType, Element, string][] {
  return Array.from(elem.children)
    .map(castToNestSourceMappingElement)
    .filter((x) => x) as [SourceMappingType, Element, string][];
}

export function removeSourceMappingHandler(docRoot: HTMLElement) {
  const prevSourceMappingHandler = (docRoot as any).sourceMappingHandler;
  if (prevSourceMappingHandler) {
    docRoot.removeEventListener("click", prevSourceMappingHandler);
    delete (docRoot as any).sourceMappingHandler;
    // console.log("remove removeSourceMappingHandler");
  }
}

export function resolveSourceLeaf(
  elem: Element,
  path: ElementPoint[],
): [Element, number] | undefined {
  const page = elem.getElementsByClassName("typst-page")[0];
  let curElem = page;

  for (const point of path.slice(1)) {
    if (point.kind === SourceMappingType.CharIndex) {
      // console.log('done char');
      return [curElem, point.index];
    }
    const children = castChildrenToSourceMappingElement(curElem);
    console.log(point, children);
    if (point.index >= children.length) {
      return undefined;
    }
    if (point.kind != children[point.index][0]) {
      return undefined;
    }
    curElem = children[point.index][1];
  }

  // console.log('done');
  return [curElem, 0];
}

export function installEditorJumpToHandler(docRoot: HTMLElement) {
  const resolveFrameLoc = async (event: MouseEvent, elem: Element) => {
    const x = event.clientX;
    const y = event.clientY;

    let mayPageElem: [SourceMappingType, Element, string] | undefined = undefined;

    while (elem) {
      mayPageElem = castToSourceMappingElement(elem);
      if (mayPageElem && mayPageElem[0] === SourceMappingType.Page) {
        break;
      }
      if (elem === docRoot) {
        return;
      }
      elem = elem.parentElement!;
    }

    if (!mayPageElem) {
      return undefined;
    }

    const pageElem = mayPageElem[1];
    console.log(mayPageElem, pageElem);

    const pageRect = pageElem.getBoundingClientRect();
    const pageX = x - pageRect.left;
    const pageY = y - pageRect.top;

    const xPercent = pageX / pageRect.width;
    const yPercent = pageY / pageRect.height;
    const pageNumber = pageElem.getAttribute("data-page-number")!;
    const dataWidthS = pageElem.getAttribute("data-page-width")!;
    const dataHeightS = pageElem.getAttribute("data-page-height")!;

    console.log(pageNumber, dataWidthS, dataHeightS);

    if (!pageNumber || !dataWidthS || !dataHeightS) {
      return undefined;
    }
    const dataWidth = Number.parseFloat(dataWidthS);
    const dataHeight = Number.parseFloat(dataHeightS);

    return {
      page_no: Number.parseInt(pageNumber) + 1,
      x: xPercent * dataWidth,
      y: yPercent * dataHeight,
    };
  };

  removeSourceMappingHandler(docRoot);
  const sourceMappingHandler = ((docRoot as any).sourceMappingHandler = async (
    event: MouseEvent,
  ) => {
    let elem = event.target! as Element;

    const frameLoc = await resolveFrameLoc(event, elem);
    if (!frameLoc) {
      return;
    }
    console.log("frameLoc", frameLoc);
    window.typstWebsocket.send(`src-point ${JSON.stringify(frameLoc)}`);

    const triggerWindow = document.body || document.firstElementChild;
    const basePos = triggerWindow.getBoundingClientRect();

    // const vw = window.innerWidth || 0;
    const left = event.clientX - basePos.left;
    const top = event.clientY - basePos.top;

    triggerRipple(
      triggerWindow,
      left,
      top,
      "typst-debug-react-ripple",
      "typst-debug-react-ripple-effect .4s linear",
    );

    return;
  });

  docRoot.addEventListener("click", sourceMappingHandler);
}

export interface TypstDebugJumpDocument {}

export function provideDebugJumpDoc<TBase extends GConstructor<TypstDocumentContext>>(
  Base: TBase,
): TBase & GConstructor<TypstDebugJumpDocument> {
  return class DebugJumpDocument extends Base {
    constructor(...args: any[]) {
      super(...args);
      if (this.opts.sourceMapping !== false) {
        installEditorJumpToHandler(this.hookedElem);
        this.disposeList.push(() => {
          if (this.hookedElem) {
            removeSourceMappingHandler(this.hookedElem);
          }
        });
      }
    }
  };
}
