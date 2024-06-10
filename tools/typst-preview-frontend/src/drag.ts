export function setupDrag() {
    let lastPos = { x: 0, y: 0 };
    let containerElement: HTMLElement | null = null;
    const mouseMoveHandler = function (e: MouseEvent) {
        // How far the mouse has been moved
        const dx = e.clientX - lastPos.x;
        const dy = e.clientY - lastPos.y;

        // Scroll the element
        window.scrollBy(0, -dy);
        containerElement?.scrollBy(-dx, 0);
        lastPos = {
            x: e.clientX,
            y: e.clientY,
        };
    };
    const mouseUpHandler = function () {
        document.removeEventListener('mousemove', mouseMoveHandler);
        document.removeEventListener('mouseup', mouseUpHandler);
        if (!containerElement) return;
        containerElement.style.cursor = 'grab';
        containerElement.style.removeProperty('user-select');
    };
    const mouseDownHandler = function (e: MouseEvent) {
        lastPos = {
            // Get the current mouse position
            x: e.clientX,
            y: e.clientY,
        };
        if (!containerElement) return;
        const elementUnderMouse = e.target as HTMLElement | null;
        if (elementUnderMouse !== null && elementUnderMouse.classList.contains('tsel')) {
            return;
        }
        containerElement.style.cursor = 'grabbing';
        containerElement.style.userSelect = 'none';

        document.addEventListener('mousemove', mouseMoveHandler);
        document.addEventListener('mouseup', mouseUpHandler);
    };
    document.addEventListener('DOMContentLoaded', () => {
        containerElement = document.getElementById('typst-container');
        if (!containerElement) return;
        containerElement.addEventListener('mousedown', mouseDownHandler);
    }
    );
}
