import van, { type State } from "vanjs-core";
import { copyToClipboard, requestTextEdit } from "@/vscode";
import type { CategorizedSymbols } from "../categorized";
import { NOPRINT_SYMBOLS, type SymbolItem, stripSymPrefix } from "../symbols";

const { div, span } = van.tags;

export const SymbolCell = (sym: SymbolItem) => {
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
    const unicode = `\\u{${sym.unicode.toString(16).toUpperCase().padStart(4, "0")}}`;
    copyToClipboard(unicode);
  };

  const fallback = () => {
    const key = stripSymPrefix(sym.id);
    return span(NOPRINT_SYMBOLS[key] ?? key);
  };

  const symbolName = stripSymPrefix(sym.id);
  const unicode = `\\u{${sym.unicode.toString(16).toUpperCase().padStart(4, "0")}}`;

  return div(
    {
      class: "symbol-cell",
      title: `Click to insert: ${symbolName}`,
      onclick: handleClick,
    },
    div({ class: "symbol-glyph" }, sym.rendered ?? fallback()),
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
      ...(cat.symbols ?? []).map((sym) => SymbolCell(sym)),
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
