import van from "vanjs-core";
import type { SymbolItem, SymbolResource } from "./symbols";

const { div } = van.tags;

export function prerenderSymbols(symRes: SymbolResource): SymbolItem[] {
  console.log("render", symRes);

  const items = symRes.symbols.map((sym) => {
    const renderedSym: SymbolItem = {
      id: sym.id,
      category: sym.category,
      unicode: sym.unicode,
      rendered: sym.glyph ? div({ class: "symbol-glyph", innerHTML: sym.glyph }) : undefined,
    };

    return renderedSym;
  });

  return items;
}
