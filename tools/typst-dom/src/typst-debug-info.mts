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
  elem: Element
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
  elem: Element
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

function castChildrenToSourceMappingElement(
  elem: Element
): [SourceMappingType, Element, string][] {
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

function findIndexOfChild(elem: Element, child: Element) {
  const children = castChildrenToSourceMappingElement(elem);
  // console.log(elem, "::", children, "=>", child);
  return children.findIndex((x) => x[1] === child);
}

export function resolveSourceLeaf(elem: Element, path: ElementPoint[]): [Element, number] | undefined {
  const page = elem.getElementsByClassName('typst-page')[0];
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

// const rotateColors = [
//   "green",
//   "blue",
//   "red",
//   "orange",
//   "purple",
//   "yellow",
//   "cyan",
//   "magenta",
// ];

function getCharIndex(elem: Element, mouseX: number, mouseY: number) {
  let useIndex = 0;
  let foundIndex = -1;
  const textRect = elem.getBoundingClientRect();

  type SelRect = Pick<DOMRect, 'left' | 'right' | 'top' | 'bottom'>;
  let previousSelRect: SelRect | undefined = undefined;
  const unionRect = (a: SelRect, b?: SelRect) => {
    if (!b) {
      return a;
    }
    return {
      left: Math.min(a.left, b.left),
      top: Math.min(a.top, b.top),
      right: Math.max(a.right, b.right),
      bottom: Math.max(a.bottom, b.bottom),
    };
  }
  const inRect = (rect: SelRect, x: number, y: number) => {
    return rect.left <= x && x <= rect.right &&
      rect.top <= y && y <= rect.bottom;
  }

  const enum TextFlowDirection {
    LeftToRight = 0,
    RightToLeft = 1,
    TopToBottom = 2,
    BottomToTop = 3,
  };

  let textFlowDir = TextFlowDirection.LeftToRight;
  const isHorizontalFlow = () => {
    return textFlowDir === TextFlowDirection.LeftToRight ||
      textFlowDir === TextFlowDirection.RightToLeft;
  }

  {
    let use0: Element = undefined!;
    let use1: Element = undefined!;
    for (const use of elem.children) {
      if (use.tagName !== 'use') {
        continue;
      }
      if (!use0) {
        use0 = use;
        continue;
      }
      use1 = use;
      break;
    }

    if (use0 && use1) {
      const use0Rect = use0.getBoundingClientRect();
      const use1Rect = use1.getBoundingClientRect();

      const use0Center = {
        x: (use0Rect.left + use0Rect.right) / 2,
        y: (use0Rect.top + use0Rect.bottom) / 2,
      };
      const use1Center = {
        x: (use1Rect.left + use1Rect.right) / 2,
        y: (use1Rect.top + use1Rect.bottom) / 2,
      };
      const vec = {
        x: use1Center.x - use0Center.x,
        y: use1Center.y - use0Center.y,
      };
      const angle = Math.atan2(vec.y, vec.x);
      // console.log('angle', angle);i
      if (angle > -Math.PI / 4 && angle < Math.PI / 4) {
        textFlowDir = TextFlowDirection.LeftToRight;
      } else if (angle < -Math.PI / 4 && angle > -Math.PI * 3 / 4) {
        textFlowDir = TextFlowDirection.TopToBottom;
      } else if (angle > Math.PI / 4 && angle < Math.PI * 3 / 4) {
        textFlowDir = TextFlowDirection.BottomToTop;
      } else {
        textFlowDir = TextFlowDirection.RightToLeft;
      }
    }
  }

  for (const use of elem.children) {
    if (use.tagName !== 'use') {
      continue;
    }
    const useRect = use.getBoundingClientRect();
    const selRect = isHorizontalFlow() ? {
      left: useRect.left,
      right: useRect.right,
      top: textRect.top,
      bottom: textRect.bottom,
    } : {
      left: textRect.left,
      right: textRect.right,
      top: useRect.top,
      bottom: useRect.bottom,
    };
    previousSelRect = unionRect(selRect, previousSelRect);

    // draw sel rect for debugging
    // const selRectElem = document.createElement('div');
    // selRectElem.style.position = 'absolute';
    // selRectElem.style.left = `${selRect.left}px`;
    // selRectElem.style.top = `${selRect.top}px`;
    // selRectElem.style.width = `${selRect.right - selRect.left}px`;
    // selRectElem.style.height = `${selRect.bottom - selRect.top}px`;
    // selRectElem.style.border = `1px solid ${rotateColors[useIndex % rotateColors.length]}`;
    // selRectElem.style.zIndex = '100';
    // document.body.appendChild(selRectElem);
    // console.log(textRect, selRect);

    // set index to end range of this char
    useIndex++;
    if (inRect(selRect, mouseX, mouseY)) {
      foundIndex = useIndex;
    } else if (previousSelRect) { // may fallback to space in between chars
      if (inRect(previousSelRect, mouseX, mouseY)) {
        foundIndex = useIndex - 1;
        previousSelRect = selRect;
      }
    }
  }

  return foundIndex;
}

export function installEditorJumpToHandler(docRoot: HTMLElement) {
  const collectElementPath = async (event: MouseEvent, elem: Element) => {
    const visitChain: [SourceMappingType, Element, string][] = [];
    while (elem) {
      let srcElem = castToSourceMappingElement(elem);
      if (srcElem) {
        if (srcElem[0] === SourceMappingType.CharIndex) {
          const textElem = elem.parentElement?.parentElement?.parentElement!;
          let foundIndex = -1;
          if (textElem) {
            foundIndex = getCharIndex(textElem, event.clientX, event.clientY);
          }
          if (foundIndex !== -1) {
            (srcElem[1] as any) = foundIndex;
            visitChain.push(srcElem);
          }
        } else {
          visitChain.push(srcElem);
        }
      }
      if (elem === docRoot) {
        break;
      }
      elem = elem.parentElement!;
    }

    if (visitChain.length === 0) {
      return undefined;
    }

    // console.log('visitChain', visitChain);

    let startIdx = 1;
    if (visitChain.length >= 1 && visitChain[0][0] === SourceMappingType.CharIndex) {
      startIdx = 2;
    }
    for (let idx = startIdx; idx < visitChain.length; idx++) {
      if (visitChain[idx - 1][0] === SourceMappingType.CharIndex) {
        throw new Error("unexpected");
      }

      const childIdx = findIndexOfChild(
        visitChain[idx][1],
        visitChain[idx - 1][1]
      );
      if (childIdx < 0) {
        return undefined;
      }
      (visitChain[idx - 1][1] as any) = childIdx;
    }

    visitChain.reverse();

    const pg = visitChain[0];
    if (pg[0] !== SourceMappingType.Page) {
      return undefined;
    }
    const childIdx = findIndexOfChild(pg[1].parentElement!, visitChain[0][1]);
    if (childIdx < 0) {
      return undefined;
    }
    (visitChain[0][1] as any) = childIdx;

    const sourceNodePath = visitChain;
    return sourceNodePath;
  };

  removeSourceMappingHandler(docRoot);
  const sourceMappingHandler = ((docRoot as any).sourceMappingHandler = async (
    event: MouseEvent
  ) => {
    let elem = event.target! as Element;

    const elementPath = await collectElementPath(event, elem);
    if (!elementPath) {
      return;
    }
    console.log("element path", elementPath);

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
      "typst-debug-react-ripple-effect .4s linear"
    );

    window.typstWebsocket.send(`srcpath ${JSON.stringify(elementPath)}`);
    return;
  });

  docRoot.addEventListener("click", sourceMappingHandler);
}

export interface TypstDebugJumpDocument {
}

export function provideDebugJumpDoc<
  TBase extends GConstructor<TypstDocumentContext>
>(Base: TBase): TBase & GConstructor<TypstDebugJumpDocument> {
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
