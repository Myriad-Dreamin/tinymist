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
const SearchInput = (searchQuery: State<string>) => () => {
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

const useArrayToggle =
  <T>(filter: State<T[]>) =>
  (key: T) => {
    const current = filter.val;
    filter.val = current.includes(key) ? current.filter((w) => w !== key) : [...current, key];
  };

const FilterToggle = (
  label: string,
  style: string,
  isActive: () => boolean,
  onclick: () => void,
) => {
  return button(
    {
      class: van.derive(() => (isActive() ? "toggle-btn active" : "toggle-btn")),
      style,
      onclick,
    },
    label,
  );
};

/**
 * Base filter component that handles common filter structure
 */
const FilterGroup = (title: string, options: ChildDom[], filter: State<string[]>) => {
  return div(
    div({ class: "text-desc" }, title),
    div(
      { class: "filter-options" },
      ...options,
      filter.val.length > 0
        ? button(
            {
              class: "btn",
              title: "Clear filters",
              onclick: () => {
                filter.val = [];
              },
            },
            "×",
          )
        : null,
    ),
  );
};

/**
 * Creates a weight filter as toggle buttons
 */
const WeightFilter = (weightFilter: State<string[]>) => {
  const toggleWeight = useArrayToggle(weightFilter);

  const options = Object.entries(FONT_WEIGHT_CATEGORIES).map(([key, category]) =>
    FilterToggle(
      `${category.label} (${category.weight})`,
      `font-weight: ${category.weight}`,
      () => weightFilter.val.includes(key),
      () => toggleWeight(key),
    ),
  );

  return FilterGroup("Weight", options, weightFilter);
};

/**
 * Creates a stretch filter as toggle buttons
 */
const StretchFilter = (stretchFilter: State<string[]>) => {
  const toggleStretch = useArrayToggle(stretchFilter);

  const options = Object.entries(FONT_STRETCH_CATEGORIES).map(([key, category]) =>
    FilterToggle(
      category.label,
      `font-stretch: ${key}`,
      () => stretchFilter.val.includes(key),
      () => toggleStretch(key),
    ),
  );

  return FilterGroup("Width", options, stretchFilter);
};

/**
 * Creates a style filter as toggle buttons
 */
const StyleFilter = (styleFilter: State<string[]>) => {
  const toggleStyle = useArrayToggle(styleFilter);

  const options = Object.entries(FONT_STYLE_CATEGORIES).map(([key, category]) =>
    FilterToggle(
      category.label,
      `font-style: ${key}`,
      () => styleFilter.val.includes(key),
      () => toggleStyle(key),
    ),
  );

  return FilterGroup("Style", options, styleFilter);
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

const ToggleButton =
  (body: ChildDom, title: string, onclick: () => void, active?: boolean) => () => {
    return button(
      {
        class: active ? "toggle-btn active" : "toggle-btn",
        title,
        onclick,
      },
      body,
    );
  };

const StatsText = (stats: State<FontStats>) => () => {
  const { filtered, total, variants } = stats.val;
  const text =
    filtered === total
      ? `Showing ${total} font families (${variants} variants)`
      : `Showing ${filtered} of ${total} font families (${variants} variants)`;
  return div({ class: "font-stats" }, text);
};

export const Header =
  (
    filterStates: FontFilterStates,
    stats: State<FontStats>,
    showNumber: State<boolean>,
    clearFilters: () => void,
  ) =>
  () => {
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
