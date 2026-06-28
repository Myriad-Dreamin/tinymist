export interface PageRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LinkTarget {
  kind: "external" | "internal" | "unknown";
  href: string;
  position?: {
    page: number;
    x: number;
    y: number;
  };
}

export interface LinkInteraction {
  rect: PageRect;
  target: LinkTarget;
}

export interface LinkTextHighlight {
  text: TextInteraction;
  rect: PageRect;
}

export interface TextInteraction {
  id: number;
  rect: PageRect;
}

export interface BoundInteraction {
  id: number;
  kind: string;
  rect: PageRect;
}

export interface PageInteractions {
  pageIndex: number;
  links: LinkInteraction[];
  texts: TextInteraction[];
}

const textHighlightEnabled = false;

const interactionTagPattern = textHighlightEnabled
  ? /<(span|a)\b([^>]*\bclass="[^"]*\btypst-content-(?:text|link)\b[^"]*"[^>]*)>/g
  : /<(span|a)\b([^>]*\bclass="[^"]*\btypst-content-link\b[^"]*"[^>]*)>/g;

/** Extracts lightweight page-space hit-test metadata from renderer semantics HTML. */
export function parsePageInteractions(pageIndex: number, html: string): PageInteractions {
  const links: LinkInteraction[] = [];
  const texts: TextInteraction[] = [];

  for (const match of html.matchAll(interactionTagPattern)) {
    const tagName = match[1];
    const attrs = match[2];
    const className = readAttribute(attrs, "class") || "";

    if (tagName === "a" || hasClass(className, "typst-content-link")) {
      const rect = readPageRect(attrs);
      if (!rect) {
        continue;
      }
      links.push({ rect, target: parseLinkTarget(attrs) });
      continue;
    }

    if (textHighlightEnabled && hasClass(className, "typst-content-text")) {
      const rect = readPageRect(attrs);
      if (!rect) {
        continue;
      }
      const id = Number.parseInt(readAttribute(attrs, "data-text-id") || "", 10);
      if (Number.isFinite(id)) {
        texts.push({ id, rect });
      }
    }
  }

  return { pageIndex, links, texts };
}

export function hitTestLink(
  interactions: PageInteractions | undefined,
  x: number,
  y: number,
): LinkInteraction | undefined {
  if (!interactions) {
    return undefined;
  }
  return findTopmost(interactions.links, x, y);
}

export function hitTestText(
  interactions: PageInteractions | undefined,
  x: number,
  y: number,
): TextInteraction | undefined {
  if (!textHighlightEnabled) {
    return undefined;
  }
  if (!interactions) {
    return undefined;
  }
  return findTopmost(interactions.texts, x, y);
}

export function intersectingTextRects(
  interactions: PageInteractions | undefined,
  rect: PageRect,
): PageRect[] {
  if (!interactions) {
    return [];
  }
  return interactions.texts
    .filter((text) => rectsIntersect(text.rect, rect))
    .map((text) => text.rect);
}

export function textRectsForLink(
  interactions: PageInteractions | undefined,
  link: LinkInteraction,
): PageRect[] {
  return textHighlightsForLink(interactions, link).map((highlight) => highlight.rect);
}

export function textHighlightsForLink(
  interactions: PageInteractions | undefined,
  link: LinkInteraction,
): LinkTextHighlight[] {
  if (!textHighlightEnabled) {
    return [];
  }
  if (!interactions) {
    return [];
  }

  const highlights: LinkTextHighlight[] = [];
  const linkRects = interactions.links
    .filter((candidate) => sameLinkTarget(candidate.target, link.target))
    .map((candidate) => visualLinkRect(candidate.rect));

  for (const linkRect of linkRects) {
    for (const text of interactions.texts) {
      const clipped = intersectRects(text.rect, linkRect);
      if (clipped && isMeaningfulTextClip(text.rect, clipped)) {
        highlights.push({ text, rect: clipped });
      }
    }
  }

  return dedupeLinkTextHighlights(highlights);
}

function findTopmost<T extends { rect: PageRect }>(
  items: T[],
  x: number,
  y: number,
): T | undefined {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    if (rectContains(items[i].rect, x, y)) {
      return items[i];
    }
  }
  return undefined;
}

function rectContains(rect: PageRect, x: number, y: number) {
  return x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height;
}

function rectsIntersect(a: PageRect, b: PageRect) {
  return a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
}

function intersectRects(a: PageRect, b: PageRect): PageRect | undefined {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  if (right <= x || bottom <= y) {
    return undefined;
  }
  return { x, y, width: right - x, height: bottom - y };
}

function visualLinkRect(rect: PageRect): PageRect {
  return insetRect(rect, Math.min(1, rect.width * 0.1), Math.min(2, rect.height * 0.2)) || rect;
}

function insetRect(rect: PageRect, xInset: number, yInset: number): PageRect | undefined {
  const width = rect.width - xInset * 2;
  const height = rect.height - yInset * 2;
  if (width <= 0 || height <= 0) {
    return undefined;
  }
  return {
    x: rect.x + xInset,
    y: rect.y + yInset,
    width,
    height,
  };
}

function isMeaningfulTextClip(text: PageRect, clipped: PageRect) {
  return clipped.width >= 0.25 && clipped.height / Math.max(text.height, 1) >= 0.45;
}

function dedupeLinkTextHighlights(highlights: LinkTextHighlight[]) {
  const seen = new Set<string>();
  return highlights.filter((highlight) => {
    const rect = highlight.rect;
    const key = `${highlight.text.id}:${rect.x.toFixed(3)}:${rect.y.toFixed(3)}:${rect.width.toFixed(3)}:${rect.height.toFixed(3)}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function sameLinkTarget(a: LinkTarget, b: LinkTarget) {
  if (a.kind !== b.kind || a.href !== b.href) {
    return false;
  }
  if (a.kind !== "internal") {
    return true;
  }
  return (
    a.position?.page === b.position?.page &&
    a.position?.x === b.position?.x &&
    a.position?.y === b.position?.y
  );
}

function readPageRect(attrs: string): PageRect | undefined {
  const fromData = {
    x: readNumberAttribute(attrs, "data-typst-x"),
    y: readNumberAttribute(attrs, "data-typst-y"),
    width: readNumberAttribute(attrs, "data-typst-width"),
    height: readNumberAttribute(attrs, "data-typst-height"),
  };
  if (isValidRect(fromData)) {
    return fromData;
  }

  const style = readAttribute(attrs, "style");
  if (!style) {
    return undefined;
  }

  const fromStyle = {
    x: readStyleCoordinate(style, "left"),
    y: readStyleCoordinate(style, "top"),
    width: readStyleCoordinate(style, "width"),
    height: readStyleCoordinate(style, "height"),
  };
  return isValidRect(fromStyle) ? fromStyle : undefined;
}

function isValidRect(rect: {
  x: number | undefined;
  y: number | undefined;
  width: number | undefined;
  height: number | undefined;
}): rect is PageRect {
  return (
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width > 0 &&
    rect.height > 0
  );
}

function parseLinkTarget(attrs: string): LinkTarget {
  const onclick = readAttribute(attrs, "onclick");
  const internal = onclick?.match(
    /handleTypstLocation\(this,\s*([0-9]+),\s*([^,\s]+),\s*([^)]+)\)/,
  );
  if (internal) {
    return {
      kind: "internal",
      href: "#",
      position: {
        page: Number.parseInt(internal[1], 10),
        x: Number.parseFloat(internal[2]),
        y: Number.parseFloat(internal[3]),
      },
    };
  }

  const href = readAttribute(attrs, "href") || "";
  if (isSupportedExternalHref(href)) {
    return { kind: "external", href };
  }
  return { kind: "unknown", href };
}

function isSupportedExternalHref(href: string) {
  return /^(https?:\/\/|mailto:)/i.test(href);
}

function readNumberAttribute(attrs: string, name: string): number | undefined {
  const value = readAttribute(attrs, name);
  if (value === undefined) {
    return undefined;
  }
  const number = Number.parseFloat(value);
  return Number.isFinite(number) ? number : undefined;
}

function readAttribute(attrs: string, name: string): string | undefined {
  const escapedName = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = attrs.match(new RegExp(`\\b${escapedName}="([^"]*)"`, "i"));
  return match ? decodeHtmlAttribute(match[1]) : undefined;
}

function hasClass(className: string, name: string) {
  return className.split(/\s+/).includes(name);
}

function readStyleCoordinate(style: string, property: string): number | undefined {
  const escapedProperty = property.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = style.match(
    new RegExp(
      `${escapedProperty}\\s*:\\s*calc\\(var\\(--data-text-(?:width|height)\\) \\* ([^)]+)\\)`,
      "i",
    ),
  );
  if (!match) {
    return undefined;
  }
  const value = Number.parseFloat(match[1]);
  return Number.isFinite(value) ? value : undefined;
}

function decodeHtmlAttribute(value: string) {
  return decodeHtmlText(value);
}

function decodeHtmlText(value: string) {
  return value
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}
