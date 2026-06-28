import type { PageLayout, PageRecord } from "./types";
import { clamp } from "./utils";

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

interface PagePoint {
  x: number;
  y: number;
}

interface PagePointer extends PagePoint {
  pageIndex: number;
}

interface ActiveTextHover {
  generation: number;
  pageIndex: number;
  rect: PageRect;
}

interface PendingTextHover extends PagePointer {
  requestId: number;
  generation: number;
  text: TextInteraction;
}

interface PendingBoundHover extends PagePointer {
  requestId: number;
  generation: number;
}

type InteractionHighlightKind = "link" | "text" | "bound";

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

function pagePointFromEvent(record: PageRecord, event: MouseEvent): PagePoint | undefined {
  const rect = record.shell.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    return undefined;
  }

  return {
    x: clamp(((event.clientX - rect.left) / rect.width) * record.width, 0, record.width),
    y: clamp(((event.clientY - rect.top) / rect.height) * record.height, 0, record.height),
  };
}

function isDragClick(
  record: PageRecord,
  pointerDown: PagePointer | undefined,
  event: MouseEvent,
) {
  if (!pointerDown || pointerDown.pageIndex !== record.index) {
    return false;
  }

  const distance = Math.hypot(event.clientX - pointerDown.x, event.clientY - pointerDown.y);
  return distance > 4;
}

function textVisualRectKey(generation: number, pageIndex: number, textId: number) {
  return `${generation}:${pageIndex}:${textId}`;
}

function textHoverRect(text: TextInteraction, hitRect: PageRect | undefined) {
  return hitRect || text.rect;
}

function rectContainsPage(rect: PageRect, x: number, y: number) {
  return x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height;
}

function alignTextClipToVisualRect(clippedRect: PageRect, visualRect: PageRect): PageRect {
  return {
    x: clippedRect.x,
    y: visualRect.y,
    width: clippedRect.width,
    height: visualRect.height,
  };
}

function renderLinkAnchors(record: PageRecord) {
  const interactions = record.interactions;
  if (!interactions) {
    record.linkLayer.replaceChildren();
    return;
  }

  const anchors = interactions.links.flatMap((link) => {
    if (link.target.kind !== "external") {
      return [];
    }

    const anchor = document.createElement("a");
    anchor.href = link.target.href;
    anchor.target = "_blank";
    anchor.rel = "noopener noreferrer";
    anchor.title = link.target.href;
    anchor.setAttribute("aria-label", link.target.href);
    anchor.addEventListener("click", (event) => event.stopPropagation());
    applyPageRectStyle(record, anchor, link.rect);
    return [anchor];
  });

  record.linkLayer.replaceChildren(...anchors);
}

function renderInteractionHighlights(
  record: PageRecord,
  rects: PageRect[],
  kind: InteractionHighlightKind,
) {
  record.interactionLayer.replaceChildren(
    ...rects.slice(0, 64).map((rect) => {
      const highlight = document.createElement("div");
      highlight.className = `typst-interaction-highlight ${kind}`;
      applyPageRectStyle(record, highlight, rect);
      return highlight;
    }),
  );
}

function clearInteractionLayer(record: PageRecord) {
  record.container.style.cursor = "";
  record.interactionLayer.replaceChildren();
}

type LinkPosition = NonNullable<LinkTarget["position"]>;

export interface PageInteractionControllerOptions {
  postWorker: (message: unknown) => void;
  getRecord: (pageIndex: number) => PageRecord | undefined;
  getRecords: () => Iterable<PageRecord>;
  getViewport: () => HTMLElement;
  isDragging: () => boolean;
  isContentPreview: () => boolean;
  scrollToTypstLocation: (position: LinkPosition) => void;
}

export class PageInteractionController {
  private generation = 0;
  private pointerDown: PagePointer | undefined;
  private lastPointer: PagePointer | undefined;
  private activeHoverKey = "";
  private activeTextHover: ActiveTextHover | undefined;
  private readonly textHoverRects = new Map<string, PageRect>();
  private readonly pendingTextRectRequests = new Set<string>();
  private readonly pendingInteractionRequests = new Set<number>();
  private nextHitRequestId = 0;
  private pendingTextHover: PendingTextHover | undefined;
  private pendingBoundHover: PendingBoundHover | undefined;

  constructor(private readonly options: PageInteractionControllerOptions) {}

  matchesGeneration(generation: number) {
    return generation === this.generation;
  }

  handleDraggingChanged(active: boolean) {
    if (!active) {
      return;
    }
    this.activeHoverKey = "";
    this.activeTextHover = undefined;
    this.pendingTextHover = undefined;
    this.pendingBoundHover = undefined;
  }

  startGeneration(generation: number) {
    if (this.generation === generation) {
      return false;
    }

    this.generation = generation;
    this.textHoverRects.clear();
    this.pendingTextRectRequests.clear();
    this.pendingInteractionRequests.clear();
    this.activeTextHover = undefined;
    this.pendingTextHover = undefined;
    this.pendingBoundHover = undefined;

    for (const record of this.options.getRecords()) {
      record.interactions = undefined;
      renderLinkAnchors(record);
    }

    return true;
  }

  removePage(pageIndex: number) {
    if (this.lastPointer?.pageIndex === pageIndex) {
      this.lastPointer = undefined;
      this.activeHoverKey = "";
    }
  }

  resetAll() {
    this.generation = 0;
    this.pointerDown = undefined;
    this.lastPointer = undefined;
    this.activeHoverKey = "";
    this.activeTextHover = undefined;
    this.textHoverRects.clear();
    this.pendingTextRectRequests.clear();
    this.pendingInteractionRequests.clear();
    this.pendingTextHover = undefined;
    this.pendingBoundHover = undefined;
  }

  installPageHandlers(record: PageRecord) {
    record.container.addEventListener("mousedown", (event) => {
      if (event.button !== 0) {
        return;
      }
      this.pointerDown = {
        pageIndex: record.index,
        x: event.clientX,
        y: event.clientY,
      };
    });

    record.container.addEventListener("mousemove", (event) => {
      this.handlePagePointerMove(record, event);
    });

    record.container.addEventListener("mouseleave", () => {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      if (this.lastPointer?.pageIndex === record.index) {
        this.lastPointer = undefined;
      }
    });

    record.container.addEventListener("click", (event) => {
      this.handlePageClick(record, event);
    });
  }

  updateInteractions(
    generation: number,
    interactions: PageInteractions[],
    invalidatedPageIndices: number[] = [],
  ) {
    if (!this.matchesGeneration(generation)) {
      return;
    }
    let refreshHover = false;
    for (const pageIndex of invalidatedPageIndices) {
      const record = this.options.getRecord(pageIndex);
      if (record) {
        record.interactions = undefined;
        renderLinkAnchors(record);
        refreshHover ||= this.lastPointer?.pageIndex === record.index;
      }
      this.pendingInteractionRequests.delete(pageIndex);
    }
    for (const pageInteractions of interactions) {
      const record = this.options.getRecord(pageInteractions.pageIndex);
      if (record) {
        record.interactions = pageInteractions;
        renderLinkAnchors(record);
        refreshHover ||= this.lastPointer?.pageIndex === record.index;
      }
      this.pendingInteractionRequests.delete(pageInteractions.pageIndex);
    }
    if (refreshHover) {
      this.activeHoverKey = "";
      this.restoreInteractionHover();
    }
  }

  markEvicted(generation: number, pageIndices: number[]) {
    if (!this.matchesGeneration(generation)) {
      return;
    }
    let refreshHover = false;
    for (const pageIndex of pageIndices) {
      const record = this.options.getRecord(pageIndex);
      if (!record) {
        continue;
      }
      record.interactions = undefined;
      renderLinkAnchors(record);
      this.pendingInteractionRequests.delete(pageIndex);
      refreshHover ||= this.lastPointer?.pageIndex === pageIndex;
    }
    if (refreshHover) {
      this.activeHoverKey = "";
      this.activeTextHover = undefined;
      this.restoreInteractionHover();
    }
  }

  handleBoundHit(message: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    bound?: BoundInteraction;
  }) {
    const pending = this.pendingBoundHover;
    if (
      !pending ||
      pending.requestId !== message.requestId ||
      pending.generation !== message.generation ||
      pending.pageIndex !== message.pageIndex ||
      !this.matchesGeneration(message.generation)
    ) {
      return;
    }

    if (
      !this.lastPointer ||
      this.lastPointer.pageIndex !== message.pageIndex ||
      Math.hypot(this.lastPointer.x - message.x, this.lastPointer.y - message.y) > 0.5
    ) {
      return;
    }

    const record = this.options.getRecord(message.pageIndex);
    if (!record) {
      return;
    }

    if (!message.bound) {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      return;
    }

    this.showBoundHover(record, message.bound);
  }

  handleTextHit(message: {
    requestId: number;
    generation: number;
    pageIndex: number;
    x: number;
    y: number;
    hit: boolean;
    rect?: PageRect;
  }) {
    const pending = this.pendingTextHover;
    if (
      !pending ||
      pending.requestId !== message.requestId ||
      pending.generation !== message.generation ||
      pending.pageIndex !== message.pageIndex ||
      !this.matchesGeneration(message.generation)
    ) {
      return;
    }

    if (
      !this.lastPointer ||
      this.lastPointer.pageIndex !== message.pageIndex ||
      Math.hypot(this.lastPointer.x - message.x, this.lastPointer.y - message.y) > 0.5
    ) {
      return;
    }

    const record = this.options.getRecord(message.pageIndex);
    if (!record) {
      return;
    }

    if (message.hit && this.textHitContains(pending.text, message.rect, message.x, message.y)) {
      this.showTextHover(record, pending.text, message.rect);
      return;
    }

    this.clearInteractionHighlight(record);
    this.activeHoverKey = "";
    this.requestBoundHover(record, { x: message.x, y: message.y });
  }

  handleTextRect(message: {
    generation: number;
    pageIndex: number;
    textId: number;
    rect?: PageRect;
  }) {
    if (!this.matchesGeneration(message.generation)) {
      return;
    }

    const key = textVisualRectKey(this.generation, message.pageIndex, message.textId);
    this.pendingTextRectRequests.delete(key);
    if (message.rect) {
      this.textHoverRects.set(key, message.rect);
    }

    if (this.lastPointer?.pageIndex === message.pageIndex) {
      this.activeHoverKey = "";
      this.restoreInteractionHover();
    }
  }

  requestViewportInteractions(layouts: PageLayout[]) {
    if (this.generation <= 0) {
      return;
    }
    if (this.options.isDragging()) {
      return;
    }

    const viewport = this.options.getViewport();
    const viewportTop = viewport.scrollTop;
    const viewportBottom = viewportTop + viewport.clientHeight;
    const pageIndices = layouts
      .filter((layout) => layout.bottom >= viewportTop && layout.top <= viewportBottom)
      .map((layout) => layout.index);
    this.requestPageInteractions(pageIndices);
  }

  private handlePagePointerMove(record: PageRecord, event: MouseEvent) {
    if (this.options.isDragging()) {
      if (this.activeHoverKey || record.interactionLayer.childElementCount > 0) {
        this.clearInteractionHighlight(record);
        this.activeHoverKey = "";
      }
      this.lastPointer = undefined;
      return;
    }

    const point = pagePointFromEvent(record, event);
    if (!point) {
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      this.lastPointer = undefined;
      return;
    }

    this.lastPointer = { pageIndex: record.index, x: point.x, y: point.y };
    this.updateInteractionHover(record, point);
  }

  private updateInteractionHover(record: PageRecord, point: PagePoint) {
    if (!record.interactions) {
      this.requestPageInteractions([record.index]);
      this.clearInteractionHighlight(record);
      this.activeHoverKey = "";
      return;
    }

    const link = hitTestLink(record.interactions, point.x, point.y);
    if (link) {
      this.showLinkHover(record, link);
      return;
    }

    if (this.activeTextHoverContains(record, point)) {
      record.container.style.cursor = "text";
      return;
    }

    const cachedText = this.cachedTextHoverAt(record, point);
    if (cachedText) {
      this.showTextHoverRect(record, cachedText.text, cachedText.rect);
      return;
    }

    const text = hitTestText(record.interactions, point.x, point.y);
    if (text) {
      this.requestTextHover(record, text, point);
      return;
    }

    this.requestBoundHover(record, point);
  }

  private restoreInteractionHover() {
    if (!this.lastPointer) {
      return;
    }
    const record = this.options.getRecord(this.lastPointer.pageIndex);
    if (!record) {
      return;
    }
    this.updateInteractionHover(record, this.lastPointer);
  }

  private requestPageInteractions(pageIndices: number[]) {
    const missing: number[] = [];
    const seen = new Set<number>();
    for (const pageIndex of pageIndices) {
      if (seen.has(pageIndex) || this.pendingInteractionRequests.has(pageIndex)) {
        continue;
      }
      seen.add(pageIndex);

      const record = this.options.getRecord(pageIndex);
      if (!record || record.interactions) {
        continue;
      }

      this.pendingInteractionRequests.add(pageIndex);
      missing.push(pageIndex);
    }

    if (missing.length === 0) {
      return;
    }

    this.options.postWorker({
      type: "request-interactions",
      generation: this.generation,
      pageIndices: missing,
    });
  }

  private handlePageClick(record: PageRecord, event: MouseEvent) {
    const wasDragClick = isDragClick(record, this.pointerDown, event);
    this.pointerDown = undefined;
    if (wasDragClick) {
      return;
    }

    const point = pagePointFromEvent(record, event);
    if (!point) {
      return;
    }
    this.lastPointer = { pageIndex: record.index, x: point.x, y: point.y };

    if (!record.interactions) {
      this.requestPageInteractions([record.index]);
    }

    const link = hitTestLink(record.interactions, point.x, point.y);
    if (link?.target.kind === "internal" && link.target.position) {
      event.preventDefault();
      this.options.scrollToTypstLocation(link.target.position);
      return;
    }

    if (this.options.isContentPreview()) {
      this.options.postWorker({ type: "send", text: `outline-sync,${record.index + 1}` });
      return;
    }

    this.options.postWorker({
      type: "send",
      text: `src-point ${JSON.stringify({
        page_no: record.index + 1,
        x: point.x,
        y: point.y,
      })}`,
    });
  }

  private showLinkHover(record: PageRecord, link: LinkInteraction) {
    const key = `link:${record.index}:${link.target.kind}:${link.target.href}:${link.rect.x}:${link.rect.y}`;
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = undefined;
    record.container.style.cursor = "pointer";

    const textRects = textHighlightsForLink(record.interactions, link).map((highlight) => {
      this.requestTextVisualRect(record, highlight.text);
      const visualRect = this.textHoverRects.get(
        textVisualRectKey(this.generation, record.index, highlight.text.id),
      );
      return visualRect ? alignTextClipToVisualRect(highlight.rect, visualRect) : highlight.rect;
    });
    renderInteractionHighlights(record, textRects, "link");
  }

  private showTextHover(record: PageRecord, text: TextInteraction, hitRect?: PageRect) {
    const visualRect = textHoverRect(text, hitRect);
    this.textHoverRects.set(textVisualRectKey(this.generation, record.index, text.id), visualRect);
    this.showTextHoverRect(record, text, visualRect);
  }

  private showTextHoverRect(record: PageRecord, text: TextInteraction, visualRect: PageRect) {
    const key = [
      "text",
      record.index,
      text.id,
      visualRect.x,
      visualRect.y,
      visualRect.width,
      visualRect.height,
    ].join(":");
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = {
      generation: this.generation,
      pageIndex: record.index,
      rect: visualRect,
    };
    record.container.style.cursor = "text";
    renderInteractionHighlights(record, [visualRect], "text");
  }

  private requestTextHover(record: PageRecord, text: TextInteraction, point: PagePoint) {
    const key = `text-query:${record.index}:${text.id}:${point.x.toFixed(1)}:${point.y.toFixed(1)}`;
    if (this.activeHoverKey === key) {
      return;
    }

    const request = {
      requestId: ++this.nextHitRequestId,
      generation: this.generation,
      pageIndex: record.index,
      x: point.x,
      y: point.y,
      text,
    };
    this.activeHoverKey = key;
    this.pendingTextHover = request;
    record.container.style.cursor = "";
    this.options.postWorker({
      type: "hit-text",
      requestId: request.requestId,
      generation: request.generation,
      pageIndex: request.pageIndex,
      x: request.x,
      y: request.y,
      rect: text.rect,
    });
  }

  private requestTextVisualRect(record: PageRecord, text: TextInteraction) {
    const key = textVisualRectKey(this.generation, record.index, text.id);
    if (this.textHoverRects.has(key) || this.pendingTextRectRequests.has(key)) {
      return;
    }
    this.pendingTextRectRequests.add(key);
    this.options.postWorker({
      type: "resolve-text-rect",
      requestId: ++this.nextHitRequestId,
      generation: this.generation,
      pageIndex: record.index,
      textId: text.id,
      rect: text.rect,
    });
  }

  private showBoundHover(record: PageRecord, bound: BoundInteraction) {
    const key = [
      "bound",
      record.index,
      bound.kind,
      bound.rect.x,
      bound.rect.y,
      bound.rect.width,
      bound.rect.height,
    ].join(":");
    if (this.activeHoverKey === key) {
      return;
    }
    this.activeHoverKey = key;
    this.activeTextHover = undefined;
    record.container.style.cursor = "";
    renderInteractionHighlights(record, [bound.rect], "bound");
  }

  private requestBoundHover(record: PageRecord, point: PagePoint) {
    const key = `bound-query:${record.index}:${point.x.toFixed(1)}:${point.y.toFixed(1)}`;
    if (this.activeHoverKey === key) {
      return;
    }

    const request = {
      requestId: ++this.nextHitRequestId,
      generation: this.generation,
      pageIndex: record.index,
      x: point.x,
      y: point.y,
    };
    this.activeHoverKey = key;
    this.pendingBoundHover = request;
    record.container.style.cursor = "";
    this.options.postWorker({
      type: "hit-bound",
      ...request,
    });
  }

  private clearInteractionHighlight(record: PageRecord) {
    this.activeTextHover = undefined;
    clearInteractionLayer(record);
  }

  private activeTextHoverContains(record: PageRecord, point: PagePoint) {
    return (
      this.activeTextHover?.generation === this.generation &&
      this.activeTextHover.pageIndex === record.index &&
      rectContainsPage(this.activeTextHover.rect, point.x, point.y)
    );
  }

  private cachedTextHoverAt(record: PageRecord, point: PagePoint) {
    const texts = record.interactions?.texts;
    if (!texts) {
      return undefined;
    }
    for (let index = texts.length - 1; index >= 0; index -= 1) {
      const text = texts[index];
      const rect = this.textHoverRects.get(
        textVisualRectKey(this.generation, record.index, text.id),
      );
      if (rect && rectContainsPage(rect, point.x, point.y)) {
        return { text, rect };
      }
    }
    return undefined;
  }

  private textHitContains(
    text: TextInteraction,
    hitRect: PageRect | undefined,
    x: number,
    y: number,
  ) {
    return rectContainsPage(textHoverRect(text, hitRect), x, y);
  }
}

function applyPageRectStyle(record: PageRecord, element: HTMLElement, rect: PageRect) {
  const x = clamp(rect.x / Math.max(record.width, 1), 0, 1);
  const y = clamp(rect.y / Math.max(record.height, 1), 0, 1);
  const right = clamp((rect.x + rect.width) / Math.max(record.width, 1), 0, 1);
  const bottom = clamp((rect.y + rect.height) / Math.max(record.height, 1), 0, 1);
  element.style.left = `${x * 100}%`;
  element.style.top = `${y * 100}%`;
  element.style.width = `${Math.max(0, right - x) * 100}%`;
  element.style.height = `${Math.max(0, bottom - y) * 100}%`;
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
    const key = [
      highlight.text.id,
      rect.x.toFixed(3),
      rect.y.toFixed(3),
      rect.width.toFixed(3),
      rect.height.toFixed(3),
    ].join(":");
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
