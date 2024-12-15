import { triggerRipple } from "typst-dom/typst-animation.mjs";

// debounce https://stackoverflow.com/questions/23181243/throttling-a-mousemove-event-to-fire-no-more-than-5-times-a-second
// ignore fast events, good for capturing double click
// @param (callback): function to be run when done
// @param (delay): integer in milliseconds
// @param (id): string value of a unique event id
// @doc (event.timeStamp): http://api.jquery.com/event.timeStamp/
// @bug (event.currentTime): https://bugzilla.mozilla.org/show_bug.cgi?id=238041
let ignoredEvent = (function () {
  let last: Record<string, any> = {},
    diff: number,
    time: number;

  return function (callback: () => void, delay: number, id: string) {
    time = new Date().getTime();
    id = id || "ignored event";
    diff = last[id] ? time - last[id] : time;

    if (diff > delay) {
      last[id] = time;
      callback();
    }
  };
})();

var overLapping = function (a: Element, b: Element) {
  var aRect = a.getBoundingClientRect();
  var bRect = b.getBoundingClientRect();

  return (
    !(
      aRect.right < bRect.left ||
      aRect.left > bRect.right ||
      aRect.bottom < bRect.top ||
      aRect.top > bRect.bottom
    ) &&
    /// determine overlapping by area
    (Math.abs(aRect.left - bRect.left) + Math.abs(aRect.right - bRect.right)) /
    Math.max(aRect.width, bRect.width) <
    0.5 &&
    (Math.abs(aRect.bottom - bRect.bottom) + Math.abs(aRect.top - bRect.top)) /
    Math.max(aRect.height, bRect.height) <
    0.5
  );
};

var searchIntersections = function (root: Element) {
  let parent = undefined,
    current = root;
  while (current) {
    if (current.classList.contains("typst-group")) {
      parent = current;
      break;
    }
    current = current.parentElement!;
  }
  if (!parent) {
    console.log("no group found");
    return;
  }
  const group = parent;
  const children = group.children;
  const childCount = children.length;

  const res = [];

  for (let i = 0; i < childCount; i++) {
    const child = children[i];
    if (!overLapping(child, root)) {
      continue;
    }
    res.push(child);
  }

  return res;
};

var getRelatedElements = function (event: any) {
  let relatedElements = event.target.relatedElements;
  if (relatedElements === undefined || relatedElements === null) {
    relatedElements = event.target.relatedElements = searchIntersections(
      event.target
    );
  }
  return relatedElements;
};

function findAncestor(el: Element, cls: string) {
  while (el && !el.classList.contains(cls)) {
    el = el.parentElement!;
  }
  return el;
}

window.initTypstSvg = function (docRoot: SVGElement) {
  /// initialize pseudo links
  var elements = docRoot.getElementsByClassName("pseudo-link");
  for (var i = 0; i < elements.length; i++) {
    let elem = elements[i] as SVGAElement;
    elem.addEventListener("mousemove", mouseMoveToLink);
    elem.addEventListener("mouseleave", mouseLeaveFromLink);
  }

  /// initialize text layout at client side
  if (false) {
    setTimeout(() => {
      layoutText(docRoot);
    }, 0);
  }

  return;

  function mouseMoveToLink(event: MouseEvent) {
    ignoredEvent(
      function () {
        const elements = getRelatedElements(event);
        if (elements === undefined || elements === null) {
          return;
        }
        for (var i = 0; i < elements.length; i++) {
          var elem = elements[i];
          if (elem.classList.contains("hover")) {
            continue;
          }
          elem.classList.add("hover");
        }
      },
      200,
      "mouse-move"
    );
  }

  function mouseLeaveFromLink(event: MouseEvent) {
    const elements = getRelatedElements(event);
    if (elements === undefined || elements === null) {
      return;
    }
    for (var i = 0; i < elements.length; i++) {
      var elem = elements[i];
      if (!elem.classList.contains("hover")) {
        continue;
      }
      elem.classList.remove("hover");
    }
  }
};

function layoutText(svg: SVGElement) {
  const divs = svg.querySelectorAll<HTMLDivElement>(".tsel");
  const canvas = document.createElementNS(
    "http://www.w3.org/1999/xhtml",
    "canvas"
  ) as HTMLCanvasElement;
  const ctx = canvas.getContext("2d")!;

  const layoutBegin = performance.now();

  for (let d of divs) {
    if (d.getAttribute("data-typst-layout-checked")) {
      continue;
    }

    if (d.style.fontSize) {
      const foreignObj = d.parentElement!;
      const innerText = d.innerText;
      const targetWidth =
        Number.parseFloat(foreignObj.getAttribute("width") || "0") || 0;
      const currentX =
        Number.parseFloat(foreignObj.getAttribute("x") || "0") || 0;
      ctx.font = `${d.style.fontSize} sans-serif`;
      const selfWidth = ctx.measureText(innerText).width;

      const scale = targetWidth / selfWidth;

      d.style.transform = `scaleX(${scale})`;
      foreignObj.setAttribute("width", selfWidth.toString());
      foreignObj.setAttribute(
        "x",
        (currentX - (selfWidth - targetWidth) * 0.5).toString()
      );

      d.setAttribute("data-typst-layout-checked", "1");
    }
  }

  console.log(`layoutText used time ${performance.now() - layoutBegin} ms`);
}

window.currentPosition = function (elem: Element) {
  const docRoot = findAncestor(elem, "typst-doc");
  if (!docRoot) {
    console.warn("no typst-doc found", elem);
    return;
  }

  interface TypstPosition {
    page: number;
    x: number;
    y: number;
    distance: number;
  }

  let result: TypstPosition | undefined = undefined;
  // The center of the window
  const cx = window.innerWidth * 0.5;
  const cy = window.innerHeight * 0.5;
  type ScrollRect = Pick<DOMRect, "left" | "top" | "width" | "height">;
  const handlePage = (pageBBox: ScrollRect, page: number) => {
    const x = pageBBox.left;
    const y = pageBBox.top;
    // console.log("page", page, x, y);

    const distance = Math.hypot(x - cx, y - cy);
    if (result === undefined || distance < result.distance) {
      result = { page, x, y, distance };
    }
  };

  const renderMode = docRoot.getAttribute("data-render-mode");
  if (renderMode === "canvas") {
    const pages = docRoot.querySelectorAll<HTMLDivElement>(".typst-page");

    for (const page of pages) {
      const pageNumber = Number.parseInt(
        page.getAttribute("data-page-number")!
      );

      const bbox = page.getBoundingClientRect();
      handlePage(bbox, pageNumber);
    }
    return result;
  }

  const children = docRoot.children;
  let nthPage = 0;
  for (let i = 0; i < children.length; i++) {
    if (children[i].tagName === "g") {
      nthPage++;
      const page = children[i] as SVGGElement;
      const bbox = page.getBoundingClientRect();
      /// Possibly a page that is not calculated yet
      if (bbox.bottom === 0 && bbox.top === 0) {
        continue;
      }
      handlePage(bbox, nthPage);
    }
  }
  return result;
};

window.handleTypstLocation = function (
  elem: Element,
  pageNo: number,
  x: number,
  y: number
) {
  const docRoot = findAncestor(elem, "typst-doc");
  if (!docRoot) {
    console.warn("no typst-doc found", elem);
    return;
  }

  type ScrollRect = Pick<DOMRect, "left" | "top" | "width" | "height">;
  const scrollTo = (pageRect: ScrollRect, innerLeft: number, innerTop: number) => {

    const windowRoot = document.body || document.firstElementChild;
    const basePos = windowRoot.getBoundingClientRect();

    const left = innerLeft - basePos.left;
    const top = innerTop - basePos.top;


    // evaluate window viewport 1vw
    const pw = window.innerWidth * 0.01;
    const ph = window.innerHeight * 0.01;

    const xOffsetInnerFix = 7 * pw;
    const yOffsetInnerFix = 38.2 * ph;

    const xOffset = left - xOffsetInnerFix;
    const yOffset = top - yOffsetInnerFix;

    const widthOccupied = 100 * 100 * pw / pageRect.width;

    const pageAdjustLeft = pageRect.left - basePos.left - 5 * pw;
    const pageAdjust = pageRect.left - basePos.left + pageRect.width - 95 * pw;

    // default single-column or multi-column layout
    if (widthOccupied >= 90 || widthOccupied < 50) {
      window.scrollTo({ behavior: "smooth", left: xOffset, top: yOffset });
    } else { // for double-column layout
      // console.log('occupied adjustment', widthOccupied, page);

      const xOffsetAdjsut = xOffset > pageAdjust ? pageAdjust : pageAdjustLeft;

      window.scrollTo({ behavior: "smooth", left: xOffsetAdjsut, top: yOffset });
    }

    // grid ripple for debug vw
    // triggerRipple(
    //   windowRoot,
    //   svgRect.left + 50 * vw,
    //   svgRect.top + 1 * vh,
    //   "typst-jump-ripple",
    //   "typst-jump-ripple-effect .4s linear",
    //   "green",
    // );

    // triggerRipple(
    //   windowRoot,
    //   pageRect.left - basePos.left + vw,
    //   pageRect.top - basePos.top + vh,
    //   "typst-jump-ripple",
    //   "typst-jump-ripple-effect .4s linear",
    //   "red",
    // );

    // triggerRipple(
    //   windowRoot,
    //   pageAdjust,
    //   pageRect.top - basePos.top + vh,
    //   "typst-jump-ripple",
    //   "typst-jump-ripple-effect .4s linear",
    //   "red",
    // );

    triggerRipple(
      windowRoot,
      left,
      top,
      "typst-jump-ripple",
      "typst-jump-ripple-effect .4s linear"
    );
  }

  const renderMode = docRoot.getAttribute("data-render-mode");
  if (renderMode === 'canvas') {
    const pages = docRoot.querySelectorAll<HTMLDivElement>('.typst-page');

    const pageMapping = new Map<number, HTMLDivElement>();
    for (const page of pages) {
      const pageNumber = Number.parseInt(page.getAttribute('data-page-number')!);
      if (pageMapping.has(pageNumber)) {
        continue;
      }
      pageMapping.set(pageNumber, page);
    }
    pageNo -= 1;

    if (!pageMapping.has(pageNo)) {
      console.warn('page not found in canvas mode', pageNo, pageMapping);
      return;
    }

    const canvasContainer = pageMapping.get(pageNo)!.firstElementChild!;
    const canvasRectBase = canvasContainer.getBoundingClientRect();
    const appliedScale = Number.parseFloat(canvasContainer.getAttribute("data-applied-scale") || "1") || 1;
    const canvasRect = {
      left: canvasRectBase.left,
      top: canvasRectBase.top,
      width: canvasRectBase.width / appliedScale,
      height: canvasRectBase.height / appliedScale,
    }

    const dataWidth =
      Number.parseFloat(canvasContainer.getAttribute("data-page-width") || "0") || 0;
    const dataHeight =
      Number.parseFloat(canvasContainer.getAttribute("data-page-height") || "0") || 0;

    const left = canvasRect.left + (x / dataWidth) * canvasRect.width;
    const top = canvasRect.top + (y / dataHeight) * canvasRect.height;

    console.log('canvas mode jump', left, top, canvasRect, dataWidth, dataHeight, x, y);

    scrollTo(canvasRect, left, top);
    return;
  }

  const children = docRoot.children;
  let nthPage = 0;
  for (let i = 0; i < children.length; i++) {
    if (children[i].tagName === "g") {
      nthPage++;
    }
    if (nthPage == pageNo) {
      const page = children[i] as SVGGElement;
      const dataWidth =
        Number.parseFloat(docRoot.getAttribute("data-width") || "0") || 0;
      const dataHeight =
        Number.parseFloat(docRoot.getAttribute("data-height") || "0") || 0;
      // console.log(page, vw, vh, x, y, dataWidth, dataHeight, docRoot);
      const svgRectBase = docRoot.getBoundingClientRect();
      const svgRect = {
        left: svgRectBase.left,
        top: svgRectBase.top,
        width: svgRectBase.width,
        height: svgRectBase.height,
      }

      const transform = page.transform.baseVal.consolidate()?.matrix;
      if (transform) {
        // console.log(transform.e, transform.f);
        svgRect.left += (transform.e / dataWidth) * svgRect.width;
        svgRect.top += (transform.f / dataHeight) * svgRect.height;
      }

      const pageRect = page.getBoundingClientRect();

      const left = svgRect.left + (x / dataWidth) * svgRect.width;
      const top = svgRect.top + (y / dataHeight) * svgRect.height;

      scrollTo(pageRect, left, top);
      return;
    }
  }
};
