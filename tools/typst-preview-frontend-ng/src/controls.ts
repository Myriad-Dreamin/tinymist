import type { PreviewElements } from "./dom";
import type { PreviewMode } from "./types";

export interface PageControlHost {
  readonly mode: PreviewMode;
  goToPreviousSlide(): void;
  goToNextSlide(): void;
  setDragging(active: boolean): void;
  toggleInvertColors(): void;
  applyWheelZoom(event: WheelEvent): void;
}

export function installPageControls(elements: PreviewElements, host: PageControlHost) {
  installKeyboardShortcuts(elements, host);
  installDragPan(elements, host);
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
          host.goToPreviousSlide();
        } else {
          handled = false;
        }
        break;
      case " ":
      case "ArrowRight":
      case "ArrowDown":
        if (host.mode === "Slide") {
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
      case "Escape":
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

function installDragPan(elements: PreviewElements, host: PageControlHost) {
  let lastX = 0;
  let lastY = 0;
  let pendingX = 0;
  let pendingY = 0;
  let animationFrame = 0;
  let moved = false;
  let dragging = false;

  const flushMouseMove = () => {
    animationFrame = 0;
    elements.viewport.scrollBy(lastX - pendingX, lastY - pendingY);
    lastX = pendingX;
    lastY = pendingY;
    moved = true;
  };

  const onMouseMove = (event: MouseEvent) => {
    event.preventDefault();
    pendingX = event.clientX;
    pendingY = event.clientY;
    if (!animationFrame) {
      animationFrame = window.requestAnimationFrame(flushMouseMove);
    }
  };

  const finishDrag = () => {
    if (!dragging) {
      return;
    }
    dragging = false;
    if (animationFrame) {
      window.cancelAnimationFrame(animationFrame);
      flushMouseMove();
    }
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", finishDrag);
    window.removeEventListener("blur", finishDrag);
    host.setDragging(false);
    if (!moved) {
      document.getSelection()?.removeAllRanges();
    }
  };

  elements.viewport.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || isInteractiveElement(event.target)) {
      return;
    }
    event.preventDefault();
    finishDrag();
    dragging = true;
    lastX = event.clientX;
    lastY = event.clientY;
    pendingX = event.clientX;
    pendingY = event.clientY;
    moved = false;
    host.setDragging(true);
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", finishDrag);
    window.addEventListener("blur", finishDrag);
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
    ? !!target.closest("button, input, select, textarea, a")
    : false;
}
