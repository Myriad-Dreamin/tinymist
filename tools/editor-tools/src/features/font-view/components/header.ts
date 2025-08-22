import van, { type ChildDom, type State } from "vanjs-core";
import {
  FONT_STRETCH_CATEGORIES,
  FONT_STYLE_CATEGORIES,
  FONT_WEIGHT_CATEGORIES,
} from "../constants";
import type { FontFilterStates, FontStats } from "../filtering";

const { input, div, button } = van.tags;

/**
 * Creates a search input component for filtering fonts
 */
const SearchInput = (searchQuery: State<string>) => {
  return input({
    class: "input flex",
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
const WeightFilter = (weightFilter: State<string>) => {
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
    div("Weight"),
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
const StretchFilter = (stretchFilter: State<string>) => {
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
    div("Width"),
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
const StyleFilter = (styleFilter: State<string>) => {
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
    div("Style"),
    div(
      { class: "filter-options" },
      ...Object.entries(FONT_STYLE_CATEGORIES).map(([key, category]) =>
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
const ClearFiltersButton = (clearFilters: () => void) => {
  return button(
    {
      class: "btn",
      style: "align-self: flex-start",
      title: "Clear all filters",
      onclick: clearFilters,
    },
    "Clear Filters",
  );
};

const ToggleButton = (
  body: ChildDom,
  title: string,
  onclick: (this: HTMLButtonElement) => void,
  active?: boolean,
) => {
  return button(
    {
      class: active ? "toggle-btn activated" : "toggle-btn",
      title,
      onclick,
    },
    body,
  );
};

const StatsText = (stats: State<FontStats>) => {
  const { filtered, total, variants } = stats.val;
  const text =
    filtered === total
      ? `Showing ${total} font families (${variants} variants)`
      : `Showing ${filtered} of ${total} font families (${variants} variants)`;
  return div({ class: "font-stats" }, text);
};

export const Header = (
  filterStates: FontFilterStates,
  stats: State<FontStats>,
  showNumber: State<boolean>,
  clearFilters: () => void,
) => {
  return div(
    { class: "font-view-header card" },
    SearchInput(filterStates.searchQuery),
    div(
      { class: "font-filters-section" },
      WeightFilter(filterStates.weightFilter),
      StretchFilter(filterStates.stretchFilter),
      StyleFilter(filterStates.styleFilter),
    ),
    ClearFiltersButton(clearFilters),
    div(
      { class: "font-stats-section" },
      StatsText(stats),
      ToggleButton(
        "Show Numbers",
        "Toggle to show weight and stretch numbers",
        () => {
          showNumber.val = !showNumber.val;
        },
        showNumber.val,
      ),
    ),
  );
};
