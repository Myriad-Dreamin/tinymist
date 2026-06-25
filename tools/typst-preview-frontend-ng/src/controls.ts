import type { PreviewElements } from "./dom";
import type { PreviewMode } from "./types";

export interface PageControlHost {
  readonly mode: PreviewMode;
  goToPreviousSlide(): void;
  goToNextSlide(): void;
  setSlidePageFromInput(value: string): void;
  focusPageSelector(): void;
  blurPageSelector(): void;
  hideHelp(): void;
  toggleHelp(): void;
  toggleInvertColors(): void;
  applyWheelZoom(event: WheelEvent): void;
}

export function installPageControls(elements: PreviewElements, host: PageControlHost) {
  elements.helpButton.addEventListener("click", () => host.toggleHelp());
  elements.pagePrev.addEventListener("click", () => host.goToPreviousSlide());
  elements.pageNext.addEventListener("click", () => host.goToNextSlide());
  elements.pageSelector.addEventListener("input", () => {
    host.setSlidePageFromInput(elements.pageSelector.value);
  });

  installKeyboardShortcuts(elements, host);
  installDragPan(elements);
  installWheelZoom(elements, host);
}

function installKeyboardShortcuts(elements: PreviewElements, host: PageControlHost) {
  window.addEventListener("keydown", (event) => {
    if (event.target instanceof HTMLInputElement && event.key !== "Escape") {
      return;
    }

    let handled = true;
    const scrollDelta = 50;
    switch (event.key) {
      case "ArrowLeft":
      case "ArrowUp":
        if (host.mode === "Slide") {
          host.blurPageSelector();
          host.hideHelp();
          host.goToPreviousSlide();
        } else {
          handled = false;
        }
        break;
      case " ":
      case "ArrowRight":
      case "ArrowDown":
        if (host.mode === "Slide") {
          host.blurPageSelector();
          host.hideHelp();
          host.goToNextSlide();
        } else {
          handled = false;
        }
        break;
      case "j":
        elements.viewport.scrollBy({ top: scrollDelta, behavior: "auto" });
        break;
      case "k":
        elements.viewport.scrollBy({ top: -scrollDelta, behavior: "auto" });
        break;
      case "h":
        elements.viewport.scrollBy({ top: -scrollDelta * 10, behavior: "smooth" });
        break;
      case "l":
        elements.viewport.scrollBy({ top: scrollDelta * 10, behavior: "smooth" });
        break;
      case "?":
        host.blurPageSelector();
        host.toggleHelp();
        break;
      case "g":
        host.hideHelp();
        host.focusPageSelector();
        break;
      case "Escape":
        host.hideHelp();
        host.blurPageSelector();
        handled = false;
        break;
      case "t":
        host.toggleInvertColors();
        break;
      default:
        handled = false;
    }

    if (handled) {
      event.preventDefault();
    }
  });
}

function installDragPan(elements: PreviewElements) {
  let lastX = 0;
  let lastY = 0;
  let moved = false;

  const onMouseMove = (event: MouseEvent) => {
    elements.viewport.scrollBy(lastX - event.clientX, lastY - event.clientY);
    lastX = event.clientX;
    lastY = event.clientY;
    moved = true;
  };

  const onMouseUp = () => {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
    elements.root.classList.remove("dragging");
    if (!moved) {
      document.getSelection()?.removeAllRanges();
    }
  };

  elements.viewport.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || isInteractiveElement(event.target)) {
      return;
    }
    event.preventDefault();
    lastX = event.clientX;
    lastY = event.clientY;
    moved = false;
    elements.root.classList.add("dragging");
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  });
}

function installWheelZoom(elements: PreviewElements, host: PageControlHost) {
  elements.viewport.addEventListener(
    "wheel",
    (event) => {
      if (!event.ctrlKey) {
        return;
      }
      event.preventDefault();
      host.applyWheelZoom(event);
    },
    { passive: false },
  );
}

function isInteractiveElement(target: EventTarget | null) {
  return target instanceof HTMLElement
    ? !!target.closest("button, input, select, textarea, a, #typst-container-top")
    : false;
}
