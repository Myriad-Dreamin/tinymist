interface TypstPosition {
  page: number;
  x: number;
  y: number;
}

interface Window {
  initTypstSvg(docRoot: SVGElement): void;
  currentPosition(elem: Element): TypstPosition | undefined;
  handleTypstLocation(elem: Element, page: number, x: number, y: number);
  typstWebsocket: WebSocket;
}
const acquireVsCodeApi: any;
