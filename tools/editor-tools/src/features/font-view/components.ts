import van, { type State } from "vanjs-core";
import { FONT_STYLE_OPTIONS, FONT_WEIGHT_CATEGORIES } from "./constants";

const { input, select, option, button } = van.tags;

/**
 * Creates a search input component for filtering fonts
 */
export const SearchInput = (searchQuery: State<string>) => {
  return input({
    class: "font-search-input",
    type: "text",
    placeholder: "Search font families or file names...",
    value: searchQuery,
    oninput: (e: Event) => {
      const target = e.target as HTMLInputElement;
      searchQuery.val = target.value;
    },
  });
};

/**
 * Creates a weight filter dropdown with value ranges
 */
export const WeightFilter = (weightFilter: State<string>) => {
  return select(
    {
      class: "font-filter-select",
      value: weightFilter,
      onchange: (e: Event) => {
        const target = e.target as HTMLSelectElement;
        weightFilter.val = target.value;
      },
    },
    option({ value: "" }, "All Weights"),
    ...Object.entries(FONT_WEIGHT_CATEGORIES).map(([key, category]) =>
      option({ value: key }, category.label),
    ),
  );
};

/**
 * Creates a stretch filter dropdown
 */
export const StretchFilter = (stretchFilter: State<string>) => {
  return select(
    {
      class: "font-filter-select",
      value: stretchFilter,
      onchange: (e: Event) => {
        const target = e.target as HTMLSelectElement;
        stretchFilter.val = target.value;
      },
    },
    option({ value: "" }, "All Widths"),
    option({ value: "condensed" }, "Condensed"),
    option({ value: "normal" }, "Normal Width"),
    option({ value: "expanded" }, "Expanded"),
  );
};

/**
 * Creates a style filter dropdown
 */
export const StyleFilter = (styleFilter: State<string>) => {
  return select(
    {
      class: "font-filter-select",
      value: styleFilter,
      onchange: (e: Event) => {
        const target = e.target as HTMLSelectElement;
        styleFilter.val = target.value;
      },
    },
    ...FONT_STYLE_OPTIONS.map(({ value, label }) => option({ value }, label)),
  );
};

/**
 * Creates a clear filters button
 */
export const ClearFiltersButton = (clearFilters: () => void) => {
  return button(
    {
      class: "filter-clear-button",
      onclick: clearFilters,
      title: "Clear all filters",
    },
    "Clear Filters",
  );
};
