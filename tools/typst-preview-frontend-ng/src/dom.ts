export interface PreviewElements {
  root: HTMLElement;
  socketState: HTMLElement;
  previewMode: HTMLElement;
  viewport: HTMLElement;
  pages: HTMLElement;
  outlinePanel: HTMLDivElement;
  emptyState: HTMLElement;
  frameKind: HTMLElement;
  frameSize: HTMLElement;
  frameCount: HTMLElement;
  byteCount: HTMLElement;
  renderCount: HTMLElement;
  lastMessage: HTMLElement;
  helpButton: HTMLButtonElement;
  helpPanel: HTMLDivElement;
  pagePrev: HTMLButtonElement;
  pageNext: HTMLButtonElement;
  pageSelector: HTMLInputElement;
  pageTotal: HTMLElement;
}

export function collectPreviewElements(): PreviewElements {
  return {
    root: requiredElement<HTMLElement>("typst-preview-ng"),
    socketState: requiredElement<HTMLElement>("socket-state"),
    previewMode: requiredElement<HTMLElement>("preview-mode"),
    viewport: requiredElement<HTMLElement>("preview-viewport"),
    pages: requiredElement<HTMLElement>("preview-pages"),
    outlinePanel: requiredElement<HTMLDivElement>("outline-panel"),
    emptyState: requiredElement<HTMLElement>("empty-state"),
    frameKind: requiredElement<HTMLElement>("frame-kind"),
    frameSize: requiredElement<HTMLElement>("frame-size"),
    frameCount: requiredElement<HTMLElement>("frame-count"),
    byteCount: requiredElement<HTMLElement>("byte-count"),
    renderCount: requiredElement<HTMLElement>("render-count"),
    lastMessage: requiredElement<HTMLElement>("last-message"),
    helpButton: requiredElement<HTMLButtonElement>("typst-top-help-button"),
    helpPanel: requiredElement<HTMLDivElement>("typst-help-panel"),
    pagePrev: requiredElement<HTMLButtonElement>("typst-page-prev-selector"),
    pageNext: requiredElement<HTMLButtonElement>("typst-page-next-selector"),
    pageSelector: requiredElement<HTMLInputElement>("typst-page-selector"),
    pageTotal: requiredElement<HTMLElement>("typst-page-total"),
  };
}

export function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`missing element #${id}`);
  }
  return element as T;
}
