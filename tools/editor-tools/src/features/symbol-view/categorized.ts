import type { DetypifySymbol } from "detypify-service";
import van, { type State } from "vanjs-core";
import {
  CATEGORY_NAMES,
  type SymbolCategory,
  type SymbolId,
  type SymbolItem,
  stripSymPrefix,
} from "./symbols";

export interface CategorizedSymbols {
  name: string;
  symbols: SymbolItem[];
}

function pickSymbolsBySearch(items: SymbolItem[], candidates: string[] | undefined): SymbolItem[] {
  if (!candidates) return items;
  return items.filter((item) => candidates.includes(item.id));
}

function pickSymbolsByDrawCandidates(
  items: SymbolItem[],
  drawCandidates: DetypifySymbol[] | undefined,
): SymbolItem[] {
  if (!drawCandidates) return items;
  if (!drawCandidates.length) return [];

  const candidateSet = new Set(drawCandidates.flatMap((it) => it.names));

  return items.filter((item) => item.id && candidateSet.has(stripSymPrefix(item.id)));
}

function categorize(
  catsRaw: Record<SymbolCategory, string>,
  symInfo: SymbolItem[],
): CategorizedSymbols[] {
  const cats = new Map(
    Object.entries(catsRaw).map(([key, name]) => [
      key as SymbolCategory,
      { name, symbols: [] as SymbolItem[] },
    ]),
  );

  for (const sym of symInfo) {
    cats.get(sym.category)?.symbols.push(sym);
  }

  return Array.from(cats.values()).filter((cat) => cat.symbols.length > 0);
}

export function useCategorizedSymbols(
  allSymbols: State<SymbolItem[]>,
  searchFilter: State<SymbolId[] | undefined>,
  drawFilter: State<DetypifySymbol[] | undefined>,
) {
  const categorized = van.derive(() => {
    return categorize(
      CATEGORY_NAMES,
      pickSymbolsBySearch(
        pickSymbolsByDrawCandidates(allSymbols.val, drawFilter.val),
        searchFilter.val,
      ),
    );
  });

  return { categorizedSymbols: categorized };
}
