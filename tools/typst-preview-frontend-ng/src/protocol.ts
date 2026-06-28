import type { InvertColorStrategy, InvertColorStrategyMap, PreviewPosition } from "./types";

export function parsePreviewPositions(text: string): PreviewPosition[] {
  return text
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .flatMap((item) => {
      const [page, x, y] = item.split(/\s+/).map((value) => Number.parseFloat(value));
      if (!Number.isFinite(page) || !Number.isFinite(x) || !Number.isFinite(y)) {
        return [];
      }
      return [{ page, x, y }];
    });
}

export function parseInvertColorStrategy(
  raw: string,
): InvertColorStrategy | InvertColorStrategyMap {
  const text = raw.trim();
  if (text === "never" || text === "auto" || text === "always") {
    return text;
  }
  try {
    return JSON.parse(text);
  } catch (_error) {
    return "never";
  }
}
