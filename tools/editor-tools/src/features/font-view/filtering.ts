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
          const selectedWeights = weightFilter.split(",").filter(Boolean);
          if (selectedWeights.length > 0) {
            const weight = info.weight ?? FONT_DEFAULTS.WEIGHT;
            const matchesAnyWeight = selectedWeights.some((weightKey) => {
              const category =
                FONT_WEIGHT_CATEGORIES[weightKey as keyof typeof FONT_WEIGHT_CATEGORIES];
              return category.weight === weight;
            });
            if (!matchesAnyWeight) return false;
          }
        }

        // Style filter
        if (styleFilter) {
          const selectedStyles = styleFilter.split(",").filter(Boolean);
          if (selectedStyles.length > 0) {
            const style = info.style || FONT_DEFAULTS.STYLE;
            const matchesAnyStyle = selectedStyles.some((selectedStyle) => {
              return selectedStyle === style || (selectedStyle === "normal" && !info.style);
            });
            if (!matchesAnyStyle) return false;
          }
        }

        // Stretch filter
        if (stretchFilter) {
          const selectedStretches = stretchFilter.split(",").filter(Boolean);
          if (selectedStretches.length > 0) {
            const stretch = info.stretch ?? FONT_DEFAULTS.STRETCH;
            const matchesAnyStretch = selectedStretches.some((stretchKey) => {
              const category =
                FONT_STRETCH_CATEGORIES[stretchKey as keyof typeof FONT_STRETCH_CATEGORIES];
              return category?.test(stretch);
            });
            if (!matchesAnyStretch) return false;
          }
        }

        return true;
      });

      return { ...family, infos: filteredVariants };
    })
    .filter((family) => family.infos.length > 0);
}
