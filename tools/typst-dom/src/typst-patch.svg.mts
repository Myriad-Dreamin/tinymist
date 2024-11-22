import {
  TypstPatchAttrs,
  changeViewPerspective,
  equalPatchElem,
  interpretTargetView,
  patchAttributes,
  runOriginViewInstructions,
} from "./typst-patch.mjs";

/// Predicate that a xml element is a `<g>` element.
function isGElem(node: Element): node is SVGGElement {
  return node.tagName === "g";
}

/// Patch the `prev <svg>` in the DOM according to `next <svg>` from the backend.
function patchRoot(prev: SVGElement, next: SVGElement) {
  /// Patch attributes
  patchAttributes(prev, next);

  /// Hard replace elements that is not a `<g>` element.
  const frozen = preReplaceNonSVGElements(prev, next);
  /// Patch `<g>` children, call `reuseOrPatchElem` to patch.
  patchChildren(prev, next);
  postReplaceNonSVGElements(prev, frozen);
  return;
}

/// apply patches to the children sequence of `prev <svg or g>` in the DOM
function patchChildren(prev: Element, next: Element) {
  const [targetView, toPatch] = interpretTargetView<SVGGElement>(
    prev.children as unknown as SVGGElement[],
    next.children as unknown as SVGGElement[],
    true,
    isGElem,
  );

  for (let [prevChild, nextChild] of toPatch) {
    reuseOrPatchElem(prevChild, nextChild);
  }

  // console.log("interpreted target view", targetView);

  const originView = changeViewPerspective(
    prev.children as unknown as SVGGElement[],
    targetView,
    isGElem
  );

  runOriginViewInstructions(prev, originView);
}

/// Replace the `prev` element with `next` element.
/// Return true if the `prev` element is reused.
/// Return false if the `prev` element is replaced.
function reuseOrPatchElem(prev: SVGGElement, next: SVGGElement) {
  const canReuse = equalPatchElem(prev, next);

  /// Even if the element is reused, we still need to replace its attributes.
  next.removeAttribute(TypstPatchAttrs.ReuseFrom);
  patchAttributes(prev, next);

  if (canReuse) {
    return true /* reused */;
  }

  /// Hard replace elements that is not a `<g>` element.
  const frozen = preReplaceNonSVGElements(prev, next);
  /// Patch `<g>` children, will call `reuseOrPatchElem` again.
  patchChildren(prev, next);
  postReplaceNonSVGElements(prev, frozen);
  return false /* reused */;
}

interface FrozenReplacement {
  inserts: Element[][];
  debug?: string;
}

function preReplaceNonSVGElements(
  prev: Element,
  next: Element
): FrozenReplacement {
  const removedIndices: number[] = [];
  const frozenReplacement: FrozenReplacement = {
    inserts: [],
    //     debug: `preReplaceNonSVGElements ${since}
    // prev: ${prev.outerHTML}
    // next: ${next.outerHTML}`
  };
  for (let i = 0; i < prev.children.length; i++) {
    const prevChild = prev.children[i];
    if (!isGElem(prevChild)) {
      removedIndices.push(i);
    }
  }

  for (const index of removedIndices.reverse()) {
    prev.children[index].remove();
  }

  let elements: Element[] = [];
  for (let i = 0; i < next.children.length; i++) {
    const nextChild = next.children[i];
    if (!isGElem(nextChild)) {
      elements.push(nextChild);
    } else {
      frozenReplacement.inserts.push(elements);
      elements = [];
    }
  }

  frozenReplacement.inserts.push(elements);

  return frozenReplacement;
}

function postReplaceNonSVGElements(prev: Element, frozen: FrozenReplacement) {
  /// Retrieve the `<g>` elements from the `prev` element.
  const gElements = Array.from(prev.children).filter(isGElem);
  if (gElements.length + 1 !== frozen.inserts.length) {
    throw new Error(`invalid frozen replacement: gElements.length (${gElements.length
      }) + 1 !=== frozen.inserts.length (${frozen.inserts.length}) ${frozen.debug || ""
      }
  current: ${prev.outerHTML}`);
  }

  /// Insert the separated elements to the `prev` element.
  for (let i = 0; i < gElements.length; i++) {
    const prevChild = gElements[i];
    for (const elem of frozen.inserts[i]) {
      prev.insertBefore(elem, prevChild);
    }
  }

  /// Append the last elements to the `prev` element.
  for (const elem of frozen.inserts[gElements.length]) {
    prev.append(elem);
  }
}

/// End of Recursive Svg Patch
/// Begin of Update to Global Svg Resources

/// the first three elements in the svg patch are common resources used by svg.
const SVG_HEADER_LENGTH = 3;

function initOrPatchSvgHeader(svg: SVGElement) {
  if (!svg) {
    throw new Error("no initial svg found");
  }

  const prevResourceHeader = document.getElementById("typst-svg-resources");
  if (prevResourceHeader) {
    patchSvgHeader(prevResourceHeader as unknown as SVGElement, svg);
    return;
  }

  /// Create a global resource header
  const resourceHeader = document.createElementNS(
    "http://www.w3.org/2000/svg",
    "svg"
  );
  resourceHeader.id = "typst-svg-resources";
  // set viewbox, width, and height
  resourceHeader.setAttribute("viewBox", "0 0 0 0");
  resourceHeader.setAttribute("width", "0");
  resourceHeader.setAttribute("height", "0");
  resourceHeader.style.opacity = "0";
  resourceHeader.style.position = "absolute";

  /// Move resources
  for (let i = 0; i < SVG_HEADER_LENGTH; i++) {
    // move ownership of elements
    resourceHeader.append(svg.firstElementChild!);
  }

  /// Insert resource header to somewhere visible to the svg element.
  document.body.prepend(resourceHeader);
}

function patchSvgHeader(prev: SVGElement, next: SVGElement) {
  for (let i = 0; i < SVG_HEADER_LENGTH; i++) {
    const prevChild = prev.children[i];
    const nextChild = next.firstElementChild!;
    nextChild.remove();

    // console.log("prev", prevChild);
    // console.log("next", nextChild);
    if (prevChild.tagName === "defs") {
      if (prevChild.getAttribute("class") === "glyph") {
        // console.log("append glyphs:", nextChild.children, "to", prevChild);
        prevChild.append(...nextChild.children);
      } else if (prevChild.getAttribute("class") === "clip-path") {
        // console.log("clip path: replace");
        // todo: gc
        prevChild.append(...nextChild.children);
      }
    } else if (
      prevChild.tagName === "style" &&
      nextChild.getAttribute("data-reuse") !== "1"
    ) {
      // console.log("replace extra style", prevChild, nextChild);

      // todo: gc
      if (nextChild.textContent) {
        // todo: looks slow
        // https://stackoverflow.com/questions/3326494/parsing-css-in-javascript-jquery
        var doc = document.implementation.createHTMLDocument(""),
          styleElement = document.createElement("style");

        styleElement.textContent = nextChild.textContent;
        // the style will only be parsed once it is added to a document
        doc.body.appendChild(styleElement);

        const currentSvgSheet = (prevChild as HTMLStyleElement).sheet!;

        let rules = new Set<string>();
        for (const rule of currentSvgSheet.cssRules) {
          rules.add(rule.cssText);
        }

        const rulesToInsert = styleElement.sheet?.cssRules || [];

        // console.log("rules to insert", currentSvgSheet, rulesToInsert);
        for (const rule of rulesToInsert) {
          if (rules.has(rule.cssText)) {
            continue;
          }
          rules.add(rule.cssText);
          currentSvgSheet.insertRule(rule.cssText);
        }
      }
    }
  }
}

/// End of Update to Global Svg Resources
/// Main

export function patchSvgToContainer(
  hookedElem: Element,
  patchStr: string,
  decorateSvgElement: (elem: SVGElement) => void = () => void 0
) {
  if (hookedElem.firstElementChild) {
    const elem = document.createElement("div");
    elem.innerHTML = patchStr;
    const next = elem.firstElementChild! as SVGElement;
    initOrPatchSvgHeader(next);
    decorateSvgElement(next);
    patchRoot(/* prev */ hookedElem.firstElementChild as SVGElement, next);
  } else {
    hookedElem.innerHTML = patchStr;
    const next = hookedElem.firstElementChild! as SVGElement;
    initOrPatchSvgHeader(next);
    decorateSvgElement(next);
  }
}
