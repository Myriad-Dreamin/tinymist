import van from "vanjs-core";
import type { SymbolItem, SymbolResource } from "./symbols";

const { div } = van.tags;

// function renderSymbol(mask: HTMLElement, glyphSvg: string) {
//   mask.style.maskImage = `url('data:image/svg+xml;utf8,${encodeURIComponent(glyphSvg)}')`;
// }

export function prerenderSymbols(symRes: SymbolResource): SymbolItem[] {
  console.log("render", symRes);
  const tasks: (() => void)[] = [];

  const items = symRes.symbols.map((sym) => {
    const renderedSym: SymbolItem = {
      id: sym.id,
      category: sym.category,
      unicode: sym.unicode,
      rendered: sym.glyph ? div({style: `mask-image: ${sym.glyph}`}) : undefined,
    };

    if (sym.glyph) {
      // push a deferred render task into the queue
      // tasks.push(() => renderSymbol(mask, sym.glyph));
    }

    return renderedSym;
  });

  // idle loop: process as many tasks as time allows
  function runTasks(deadline: IdleDeadline) {
    while ((deadline.timeRemaining() > 0 || deadline.didTimeout) && tasks.length > 0) {
      const task = tasks.shift();
      task?.();
    }
    if (tasks.length > 0) {
      requestIdleCallback(runTasks);
    }
  }

  requestIdleCallback(runTasks);

  return items;
}
