import MiniSearch from "minisearch";
import van, { type State } from "vanjs-core";
import { CATEGORY_NAMES, type SymbolId, type SymbolItem, stripSymPrefix } from "./symbols";

export function useSymbolSearch(symbols: State<SymbolItem[]>) {
  const defaultTokenizer = MiniSearch.getDefault("tokenize");

  const search = van.derive(() => {
    const search = new MiniSearch<SymbolItem>({
      fields: ["id", "category", "categoryHuman"],
      storeFields: ["id"],
      extractField: (sym, fieldName) => {
        if (fieldName === "categoryHuman") {
          return CATEGORY_NAMES[sym.category];
        }
        return sym[fieldName as keyof SymbolItem] as string;
      },
      tokenize: (text, fieldName) => {
        if (fieldName === "id") {
          return stripSymPrefix(text).split(".");
        } else {
          return defaultTokenizer(text);
        }
      },
    });
    search.addAll(symbols.val);
    return search;
  });

  const filteredSymbols = van.state<SymbolId[] | undefined>();

  const updateFilter = (value: string) => {
    if (value === "") {
      filteredSymbols.val = undefined;
      return;
    }
    const results = search.val.search(value, { prefix: true });
    filteredSymbols.val = results.map((r) => r.id);
  };

  return {
    filteredSymbols,
    updateFilter,
  };
}
