import type { PreviewPosition } from "./types";

export function renderOutline(
  panel: HTMLDivElement,
  outlineData: any,
  contentPreview: boolean,
  scrollToTypstLocation: (position: PreviewPosition) => void,
) {
  const items = Array.isArray(outlineData?.items) ? outlineData.items : [];
  panel.replaceChildren();
  panel.classList.toggle("hidden", !contentPreview || items.length === 0);
  if (!contentPreview || items.length === 0) {
    return;
  }

  const fragment = document.createDocumentFragment();
  for (const item of items) {
    fragment.appendChild(createOutlineItem(item, 1, scrollToTypstLocation));
  }
  panel.appendChild(fragment);
}

function createOutlineItem(
  item: any,
  level: number,
  scrollToTypstLocation: (position: PreviewPosition) => void,
): HTMLElement {
  const container = document.createElement("div");
  container.className = `typst-outline level-${Math.min(level, 5)}`;
  const title = document.createElement("button");
  title.type = "button";
  title.className = "typst-outline-title";
  title.textContent = String(item?.title ?? "");

  const position = outlinePosition(item?.position);
  if (position) {
    title.addEventListener("click", () => scrollToTypstLocation(position));
  }

  container.appendChild(title);
  for (const child of Array.isArray(item?.children) ? item.children : []) {
    container.appendChild(createOutlineItem(child, level + 1, scrollToTypstLocation));
  }
  return container;
}

function outlinePosition(position: any): PreviewPosition | undefined {
  if (!position) {
    return undefined;
  }
  return {
    page: Number(position.page_no ?? position.page ?? 1),
    x: Number(position.x ?? 0),
    y: Number(position.y ?? 0),
  };
}
