import type { PageRecord, PreviewPosition } from "./types";
import { clamp } from "./utils";

export function parseCursorPosition(text: string): PreviewPosition | undefined {
  const [page, x, y] = text
    .trim()
    .split(/\s+/)
    .map((value) => Number.parseFloat(value));
  if (!Number.isFinite(page) || !Number.isFinite(x) || !Number.isFinite(y)) {
    return undefined;
  }
  return { page, x, y };
}

export function scrollViewportToTypstLocation(
  viewport: HTMLElement,
  record: PageRecord,
  position: PreviewPosition,
) {
  const xRatio = clamp(position.x / Math.max(record.width, 1), 0, 1);
  const yRatio = clamp(position.y / Math.max(record.height, 1), 0, 1);
  const left = record.container.offsetLeft + xRatio * record.container.offsetWidth;
  const top = record.container.offsetTop + yRatio * record.container.offsetHeight;
  viewport.scrollTo({
    left: Math.max(0, left - viewport.clientWidth / 2),
    top: Math.max(0, top - viewport.clientHeight / 2),
    behavior: "auto",
  });
  return { xRatio, yRatio };
}

export function showJumpMarker(record: PageRecord, xRatio: number, yRatio: number) {
  record.jumpMarker.style.left = `${xRatio * 100}%`;
  record.jumpMarker.style.top = `${yRatio * 100}%`;
  record.jumpMarker.classList.remove("visible");
  void record.jumpMarker.offsetWidth;
  record.jumpMarker.classList.add("visible");
  window.setTimeout(() => record.jumpMarker.classList.remove("visible"), 900);
}

export function renderCursor(
  records: Iterable<PageRecord>,
  pendingCursor: PreviewPosition | undefined,
) {
  const recordList = Array.from(records);
  for (const record of recordList) {
    record.cursor.classList.remove("visible");
  }
  if (!pendingCursor) {
    return;
  }

  for (const record of recordList) {
    if (record.index !== pendingCursor.page - 1) {
      continue;
    }
    record.cursor.style.left = `${
      clamp(pendingCursor.x / Math.max(record.width, 1), 0, 1) * 100
    }%`;
    record.cursor.style.top = `${
      clamp(pendingCursor.y / Math.max(record.height, 1), 0, 1) * 100
    }%`;
    record.cursor.classList.add("visible");
    return;
  }
}
