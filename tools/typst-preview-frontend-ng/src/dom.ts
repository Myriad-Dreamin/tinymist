export interface PreviewElements {
  root: HTMLElement;
  viewport: HTMLElement;
  pages: HTMLElement;
  outlinePanel: HTMLDivElement;
}

export function collectPreviewElements(): PreviewElements {
  return {
    root: requiredElement<HTMLElement>("typst-preview-ng"),
    viewport: requiredElement<HTMLElement>("preview-viewport"),
    pages: requiredElement<HTMLElement>("preview-pages"),
    outlinePanel: requiredElement<HTMLDivElement>("outline-panel"),
  };
}

export function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`missing element #${id}`);
  }
  return element as T;
}
