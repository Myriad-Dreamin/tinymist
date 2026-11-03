import { PreviewMode } from "typst-dom/typst-doc.mjs";

export type PreviewKeyAction =
  | "prev"
  | "next"
  | "scrollDown"
  | "scrollUp"
  | "scrollPageUp"
  | "scrollPageDown"
  | "toggleHelp"
  | "focusInput"
  | "escape"
  | "toggleTheme";

export interface PreviewKeyEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

export interface PreviewKeyResult {
  action: PreviewKeyAction | null;
  preventDefault: boolean;
}

const NONE: PreviewKeyResult = { action: null, preventDefault: false };

export function resolvePreviewKey(e: PreviewKeyEvent, previewMode: PreviewMode): PreviewKeyResult {
  if (e.ctrlKey || e.metaKey || e.altKey) {
    return NONE;
  }

  const inSlide = previewMode === PreviewMode.Slide;

  switch (e.key) {
    case "ArrowLeft":
    case "ArrowUp":
      return { action: inSlide ? "prev" : null, preventDefault: true };
    case " ":
    case "ArrowRight":
    case "ArrowDown":
      return { action: inSlide ? "next" : null, preventDefault: true };
    case "j":
      return { action: "scrollDown", preventDefault: true };
    case "k":
      return { action: "scrollUp", preventDefault: true };
    case "h":
      return { action: "scrollPageUp", preventDefault: true };
    case "l":
      return { action: "scrollPageDown", preventDefault: true };
    case "?":
      return { action: "toggleHelp", preventDefault: true };
    case "g":
      return { action: "focusInput", preventDefault: true };
    case "Escape":
      return { action: "escape", preventDefault: false };
    case "t":
      return { action: "toggleTheme", preventDefault: true };
    default:
      return NONE;
  }
}
