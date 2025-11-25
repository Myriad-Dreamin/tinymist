import van, { type State } from "vanjs-core";
import { copyToClipboard, requestTextEdit } from "@/vscode";
import type { CategorizedSymbols } from "../categorized";
import { NOPRINT_SYMBOLS, type SymbolItem, stripSymPrefix } from "../symbols";

const { div, span } = van.tags;

const cachedCells = new Map<string, HTMLElement>();

export const SymbolCell = (sym: SymbolItem) => {
  if (cachedCells.has(sym.id)) {
    return cachedCells.get(sym.id);
  }

  const handleClick = () => {
    const code = sym.id;
    requestTextEdit({
      newText: {
        kind: "by-mode",
        math: stripSymPrefix(code),
        markup: `#${code}`,
        rest: code,
      },
    });
  };

  const handleNameClick = (e: Event) => {
    e.stopPropagation();
    const symbolName = stripSymPrefix(sym.id);
    copyToClipboard(symbolName);
  };

  const handleUnicodeClick = (e: Event) => {
    e.stopPropagation();
    copyToClipboard(unicode);
  };

  const fallback = () => {
    const key = stripSymPrefix(sym.id);
    return span({ class: "symbol-glyph" }, NOPRINT_SYMBOLS[key] ?? key);
  };

  const symbolName = stripSymPrefix(sym.id);
  const unicode = Array.from(sym.value)
    .map((c) => {
      // biome-ignore lint/style/noNonNullAssertion: The codePointAt will always return a value for a non-empty string
      const u = c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");
      return `\\u{${u}}`;
    })
    .join("");

  const elem = div(
    {
      class: "symbol-cell",
      title: `Click to insert: ${symbolName}`,
      onclick: handleClick,
    },
    (sym.glyph && div({ class: "symbol-glyph", innerHTML: sym.glyph })) ?? fallback(),
    div(
      { class: "symbol-details" },
      div(
        {
          class: "symbol-name clickable",
          title: `Click to copy: ${symbolName}`,
          onclick: handleNameClick,
        },
        symbolName,
      ),
      div(
        {
          class: "symbol-unicode clickable",
          title: `Click to copy: ${unicode}`,
          onclick: handleUnicodeClick,
        },
        unicode,
      ),
    ),
  );
  cachedCells.set(sym.id, elem);
  return elem;
};

export const CategoryPicker = (cat: CategorizedSymbols) => {
  return div(
    div(
      {
        class: "text-lg font-bold",
        style: "margin: 0.5rem 0 0.25rem 0",
      },
      cat.name,
    ),
    div(
      { class: "symbol-grid flex-row flex-wrap gap-xs" },
      ...cat.symbols.map((sym) => SymbolCell(sym)),
    ),
  );
};

export const SymbolPicker = (
  categorizedSymbols: State<CategorizedSymbols[]>,
  showSymbolDetails: State<boolean>,
) => {
  return div(
    { class: () => `symbol-picker flex-1 ${showSymbolDetails.val ? "detailed" : ""}` },
    () => div(...categorizedSymbols.val.map((cate) => CategoryPicker(cate))),
  );
};
