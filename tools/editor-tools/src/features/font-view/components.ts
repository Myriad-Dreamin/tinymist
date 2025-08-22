import van, { type State } from "vanjs-core";
import { FONT_STRETCH_CATEGORIES, FONT_STYLE_OPTIONS, FONT_WEIGHT_CATEGORIES } from "./constants";

const { input, div, button } = van.tags;

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
 * Creates a weight filter as toggle buttons
 */
export const WeightFilter = (weightFilter: State<string>) => {
  const selectedWeights = van.derive(() =>
    weightFilter.val ? weightFilter.val.split(",").filter(Boolean) : [],
  );

  const toggleWeight = (key: string) => {
    const current = selectedWeights.val;
    const newSelection = current.includes(key)
      ? current.filter((w) => w !== key)
      : [...current, key];
    weightFilter.val = newSelection.join(",");
  };

  return div(
    { class: "filter-group" },
    div({ class: "filter-label" }, "Weight"),
    div(
      { class: "filter-options" },
      ...Object.entries(FONT_WEIGHT_CATEGORIES).map(([key, category]) =>
        button(
          {
            class: van.derive(() =>
              selectedWeights.val.includes(key)
                ? "filter-toggle-button active"
                : "filter-toggle-button",
            ),
            style: `font-weight: ${category.weight}`,
            onclick: () => toggleWeight(key),
          },
          `${category.label} (${category.weight})`,
        ),
      ),
    ),
  );
};

/**
 * Creates a stretch filter as toggle buttons
 */
export const StretchFilter = (stretchFilter: State<string>) => {
  const selectedStretches = van.derive(() =>
    stretchFilter.val ? stretchFilter.val.split(",").filter(Boolean) : [],
  );

  const toggleStretch = (key: string) => {
    const current = selectedStretches.val;
    const newSelection = current.includes(key)
      ? current.filter((s) => s !== key)
      : [...current, key];
    stretchFilter.val = newSelection.join(",");
  };

  return div(
    { class: "filter-group" },
    div({ class: "filter-label" }, "Width"),
    div(
      { class: "filter-options" },
      ...Object.entries(FONT_STRETCH_CATEGORIES).map(([key, category]) =>
        button(
          {
            class: van.derive(() =>
              selectedStretches.val.includes(key)
                ? "filter-toggle-button active"
                : "filter-toggle-button",
            ),
            style: `font-stretch: ${key}`,
            onclick: () => toggleStretch(key),
          },
          category.label,
        ),
      ),
    ),
  );
};

/**
 * Creates a style filter as toggle buttons
 */
export const StyleFilter = (styleFilter: State<string>) => {
  const selectedStyles = van.derive(() =>
    styleFilter.val ? styleFilter.val.split(",").filter(Boolean) : [],
  );

  const toggleStyle = (value: string) => {
    if (!value) return; // Skip "All Styles" option

    const current = selectedStyles.val;
    const newSelection = current.includes(value)
      ? current.filter((s) => s !== value)
      : [...current, value];
    styleFilter.val = newSelection.join(",");
  };

  return div(
    { class: "filter-group" },
    div({ class: "filter-label" }, "Style"),
    div(
      { class: "filter-options" },
      ...Object.entries(FONT_STYLE_OPTIONS).map(([key, category]) =>
        button(
          {
            class: van.derive(() =>
              selectedStyles.val.includes(key)
                ? "filter-toggle-button active"
                : "filter-toggle-button",
            ),
            style: `font-style: ${key}`,
            onclick: () => toggleStyle(key),
          },
          category.label,
        ),
      ),
    ),
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
