import { FONT_DEFAULTS, FONT_STRETCH_CATEGORIES, FONT_WEIGHT_CATEGORIES } from "./constants";
import type { FontFamily, FontFilters, FontResources } from "./types";

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
        if (weightFilter) {
          const weight = info.weight ?? FONT_DEFAULTS.WEIGHT;
          const category =
            FONT_WEIGHT_CATEGORIES[weightFilter as keyof typeof FONT_WEIGHT_CATEGORIES];
          if (category && !category.test(weight)) return false;
        }

        // Style filter
        if (styleFilter) {
          const style = info.style || FONT_DEFAULTS.STYLE;
          if (styleFilter !== style && !(styleFilter === "normal" && !info.style)) {
            return false;
          }
        }

        // Stretch filter
        if (stretchFilter) {
          const stretch = info.stretch ?? FONT_DEFAULTS.STRETCH;
          const category =
            FONT_STRETCH_CATEGORIES[stretchFilter as keyof typeof FONT_STRETCH_CATEGORIES];
          if (category && !category.test(stretch)) return false;
        }

        return true;
      });

      return { ...family, infos: filteredVariants };
    })
    .filter((family) => family.infos.length > 0);
}
