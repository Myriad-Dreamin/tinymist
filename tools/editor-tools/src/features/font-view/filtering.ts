import van, { type State } from "vanjs-core";
import { FONT_DEFAULTS, FONT_STRETCH_CATEGORIES, FONT_WEIGHT_CATEGORIES } from "./constants";
import type { FontFamily, FontResources } from "./fonts";

export interface FontFilters {
  searchQuery: string;
  weightFilter: string[];
  styleFilter: string[];
  stretchFilter: string[];
}

export interface FontFilterStates {
  searchQuery: State<string>;
  weightFilter: State<string[]>;
  styleFilter: State<string[]>;
  stretchFilter: State<string[]>;
}

export interface FontStats {
  total: number;
  filtered: number;
  variants: number;
}

export function filterFontFamilies(
  fontResources: FontResources,
  filters: FontFilters,
): FontFamily[] {
  const { searchQuery, weightFilter, styleFilter, stretchFilter } = filters;

  // No filters active
  if (!searchQuery && !weightFilter && !styleFilter && !stretchFilter) {
    return fontResources.families;
  }

  return fontResources.families
    .filter((family) => {
      // First, check if family matches search query (if any)
      if (!searchQuery) {
        return true;
      }

      const query = searchQuery.toLowerCase();

      // Check family name
      const matchesFamilyName = () => family.name.toLowerCase().includes(query);

      // Check file names in any variant
      const matchesFileName = () =>
        family.infos.some((info) => {
          if (typeof info.source !== "number") return false;
          const source = fontResources.sources[info.source];
          if (!source) return false;

          const fileName =
            source.kind === "fs" ? source.path.split(/[\\/]/).pop() || "" : source.name;
          return fileName.toLowerCase().includes(query);
        });

      return matchesFamilyName() || matchesFileName();
    })
    .map((family) => {
      // Apply variant filtering
      const filteredVariants = family.infos.filter((info) => {
        // Weight filter
        if (weightFilter.length > 0) {
          const weight = info.weight ?? FONT_DEFAULTS.WEIGHT;
          const matchesAnyWeight = weightFilter.some((weightKey) => {
            const category =
              FONT_WEIGHT_CATEGORIES[weightKey as keyof typeof FONT_WEIGHT_CATEGORIES];
            return category?.weight === weight;
          });
          if (!matchesAnyWeight) return false;
        }

        // Style filter
        if (styleFilter.length > 0) {
          const style = info.style ?? FONT_DEFAULTS.STYLE;
          const matchesAnyStyle = styleFilter.some((selectedStyle) => {
            return selectedStyle === style || (selectedStyle === "normal" && !info.style);
          });
          if (!matchesAnyStyle) return false;
        }

        // Stretch filter
        if (stretchFilter.length > 0) {
          const stretch = info.stretch ?? FONT_DEFAULTS.STRETCH;
          const matchesAnyStretch = stretchFilter.some((stretchKey) => {
            const category =
              FONT_STRETCH_CATEGORIES[stretchKey as keyof typeof FONT_STRETCH_CATEGORIES];
            return category?.test(stretch);
          });
          if (!matchesAnyStretch) return false;
        }

        return true;
      });

      return { ...family, infos: filteredVariants };
    })
    .filter((family) => family.infos.length > 0);
}

export function useFontFilters(fontResources: State<FontResources>) {
  const fontFilters: FontFilterStates = {
    searchQuery: van.state(""),
    weightFilter: van.state([]),
    styleFilter: van.state([]),
    stretchFilter: van.state([]),
  };

  const clearFilters = () => {
    fontFilters.searchQuery.val = "";
    fontFilters.weightFilter.val = [];
    fontFilters.styleFilter.val = [];
    fontFilters.stretchFilter.val = [];
  };

  const filteredFamilies = van.derive(() => {
    try {
      return filterFontFamilies(fontResources.val, {
        searchQuery: fontFilters.searchQuery.val,
        weightFilter: fontFilters.weightFilter.val,
        styleFilter: fontFilters.styleFilter.val,
        stretchFilter: fontFilters.stretchFilter.val,
      });
    } catch (error) {
      console.error("Error filtering font families:", error);
      return [];
    }
  });

  const fontStats = van.derive(() => {
    const total = fontResources.val.families.length;
    const filtered = filteredFamilies.val.length;
    const variants = filteredFamilies.val.reduce(
      (sum: number, family: FontFamily) => sum + family.infos.length,
      0,
    );

    return { total, filtered, variants };
  });

  return { fontFilters, clearFilters, filteredFamilies, fontStats };
}
