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
  numberTheory: "Number Theory",
  algebra: "algebra",
  geometry: "Geometry",
  currency: "Currency",
  shape: "Shape",
  arrow: "Arrow",
  harpoon: "Harpoon",
  tack: "Tack",
  greek: "Greek Letters",
  hebrew: "Hebrew Letters",
  doubleStruck: "Double Struck",
  // mathsConstruct: "Maths Constructs",
  // variableSizedSymbol: "Variable-sized symbols",
  // operator: "Operators and Relations",
  misc: "Miscellaneous",
  // emoji: "Emoji",
  // letterStyle: "Letter Styles",
}; // note: commented ones are not used in upstream

export type SymbolCategory = keyof typeof CATEGORY_NAMES;

export const NOPRINT_SYMBOLS: { [key: string]: string } = {
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

export interface GlyphDesc {
  fontIndex: number | null;
  xAdvance?: number | null;
  yAdvance?: number | null;
  xMin?: number | null;
  yMin?: number | null;
  xMax?: number | null;
  yMax?: number | null;
  name?: string | null;
  shape?: string | null;
}

export type SymbolId = string;

export interface SymbolItem {
  id: SymbolId;
  category: SymbolCategory;
  unicode: number;
  rendered?: HTMLElement;
}

export interface FontItem {
  family: string;
  capHeight: number;
  ascender: number;
  descender: number;
  unitsPerEm: number;
}

export interface RawSymbolItem {
  category: SymbolCategory;
  unicode: number;
  glyphs: GlyphDesc[];
}

export interface SymbolResource {
  symbols: Record<string, RawSymbolItem>;
  fontSelects: FontItem[];
  glyphDefs: Record<string, string>;
}

export function stripSymPrefix(name: string): string {
  return name.startsWith("sym.") ? name.slice(4) : name;
}
