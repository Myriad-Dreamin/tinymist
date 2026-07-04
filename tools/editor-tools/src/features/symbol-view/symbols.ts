export const CATEGORY_NAMES = {
  control: "Control",
  space: "Space",
  delimiter: "Delimiters",
  punctuation: "Punctuations",
  accent: "Accents",
  quote: "Quotes",
  prime: "Primes",
  arithmetic: "Arithmetic operators",
  relation: "Relation operators",
  setTheory: "Set Theory",
  calculus: "Calculus",
  logic: "Logic",
  functionAndCategoryTheory: "Function and category theory",
  gameTheory: "Game Theory",
  numberTheory: "Number Theory",
  algebra: "algebra",
  geometry: "Geometry",
  astronomical: "Astronomical",
  currency: "Currency",
  music: "Music",
  shape: "Shape",
  arrow: "Arrows, harpoons, and tacks",
  greek: "Greek Letters",
  cyrillic: "Cyrillic Letters",
  hebrew: "Hebrew Letters",
  doubleStruck: "Double Struck",
  miscellany: "Miscellany",
  misc: "Miscellaneous",
};

export type SymbolCategory = keyof typeof CATEGORY_NAMES;

export const NOPRINT_SYMBOLS: Record<string, string> = {
  space: "␣",
  "space.en": "ensp",
  "space.quad": "emsp",
  "space.fig": "numsp",
  "space.punct": "punctsp",
  "space.thin": "thinsp",
  "space.hair": "hairsp",
  "space.nobreak": "nbsp",
  "space.med": "mmsp",
  "space.nobreak.narrow": "",
  "space.third": "⅓emsp",
  "space.quarter": "¼emsp",
  "space.sixth": "⅙emsp",
  zws: "zwsp",
};

export type SymbolId = string;

export interface SymbolItem {
  id: SymbolId;
  category: SymbolCategory;
  value: string;
  glyph?: string;
}

export interface SymbolResource {
  symbols: SymbolItem[];
}

export function stripSymPrefix(name: string): string {
  return name.startsWith("sym.") ? name.slice(4) : name;
}
