export function setupDrag() {
  let lastPos = { x: 0, y: 0 };
  let moved = false;
  let containerElement: HTMLElement | null = null;
  const mouseMoveHandler = function (e: MouseEvent) {
    // How far the mouse has been moved
    const dx = e.clientX - lastPos.x;
    const dy = e.clientY - lastPos.y;

    window.scrollBy(-dx, -dy);
    lastPos = {
      x: e.clientX,
      y: e.clientY,
    };
    moved = true;
  };
  const goodDrag = (element: HTMLElement | null): element is HTMLElement => {
    if (!element) return false;
    // is not child of id=typst-container-top
    while (element) {
      if (element.id === "typst-container-top") {
        return false;
      }
      element = element.parentElement;
    }
    return true;
  };
  const mouseUpHandler = function () {
    document.removeEventListener("mousemove", mouseMoveHandler);
    document.removeEventListener("mouseup", mouseUpHandler);
    if (!goodDrag(containerElement)) return;
    if (!moved) {
      document.getSelection()?.removeAllRanges();
    }
    containerElement.style.cursor = "grab";
  };
  const mouseDownHandler = function (e: MouseEvent) {
    lastPos = {
      // Get the current mouse position
      x: e.clientX,
      y: e.clientY,
    };
    if (!goodDrag(containerElement)) return;
    const elementUnderMouse = e.target as HTMLElement | null;
    if (elementUnderMouse !== null && elementUnderMouse.classList.contains("tsel")) {
      return;
    }
    e.preventDefault();
    containerElement.style.cursor = "grabbing";
    moved = false;

    document.addEventListener("mousemove", mouseMoveHandler);
    document.addEventListener("mouseup", mouseUpHandler);
  };
  document.addEventListener("DOMContentLoaded", () => {
    containerElement = document.getElementById("typst-container");
    if (!containerElement) return;
    containerElement.addEventListener("mousedown", mouseDownHandler);
  });
}
