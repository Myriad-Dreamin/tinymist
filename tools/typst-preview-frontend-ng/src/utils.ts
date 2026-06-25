import type { PreviewMode } from "./types";

export function acquireVscodeApi() {
  try {
    if (typeof acquireVsCodeApi === "function") {
      return acquireVsCodeApi();
    }
  } catch (_error) {
    return undefined;
  }
  return undefined;
}

export function parsePreviewMode(value: string | undefined): PreviewMode {
  const mode = String(value || "preview-arg:previewMode:Doc").replace(
    "preview-arg:previewMode:",
    "",
  );
  return mode === "Slide" ? "Slide" : "Doc";
}

export function parsePreviewState(value: string | undefined) {
  const prefix = "preview-arg:state:";
  const encoded = String(value || "").startsWith(prefix) ? String(value).slice(prefix.length) : "";
  if (!encoded) {
    return undefined;
  }

  try {
    const binary = atob(encoded);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes));
  } catch (error) {
    console.warn("[typst-preview-ng] failed to decode preview state", error);
    return undefined;
  }
}

export function resolveWebSocketUrl(value: string | undefined) {
  const trimmed = String(value || "").trim();
  if (!trimmed) {
    return "";
  }
  const url = new URL(trimmed || "ws://127.0.0.1:23625", window.location.href);
  if (url.protocol === "http:") {
    url.protocol = "ws:";
  } else if (url.protocol === "https:") {
    url.protocol = "wss:";
  } else if (location.protocol === "https:" && url.protocol === "ws:") {
    url.protocol = "wss:";
  }
  return url.href;
}

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
