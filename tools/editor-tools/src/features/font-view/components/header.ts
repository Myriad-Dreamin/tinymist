import van, { type ChildDom, type State } from "vanjs-core";
import type { FontFilterStates, FontStats } from "../filtering";
import { FONT_STRETCH_CATEGORIES, FONT_STYLE_CATEGORIES, FONT_WEIGHT_CATEGORIES } from "../fonts";

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

/**
 * Base filter component that handles common filter structure
 */
const FilterGroup = (
  filter: State<string[]>,
  title: string,
  options: { key: string; label: string; style: string }[],
) => {
  const toggle = useArrayToggle(filter);

  return div(
    { class: "flex gap-xs" },
    div({ class: "text-sm", style: "min-width: 3rem" }, title),
    div(
      { class: "flex flex-wrap gap-xs" },
      ...options.map(({ key, label, style }) =>
        button(
          {
            class: filter.val.includes(key) ? "toggle-btn active" : "toggle-btn",
            style,
            title: style,
            onclick: () => toggle(key),
          },
          label,
        ),
      ),
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
const WeightFilter = (weightFilter: State<string[]>, showNumber: State<boolean>) => {
  return FilterGroup(
    weightFilter,
    "Weight",
    Object.entries(FONT_WEIGHT_CATEGORIES).map(([key, category]) => ({
      key,
      label: showNumber.val ? `${category.label} (${category.value})` : category.label,
      style: `font-weight: ${key}`,
    })),
  );
};

/**
 * Creates a stretch filter as toggle buttons
 */
const StretchFilter = (stretchFilter: State<string[]>) => {
  return FilterGroup(
    stretchFilter,
    "Stretch",
    Object.entries(FONT_STRETCH_CATEGORIES).map(([key, category]) => ({
      key,
      label: category.label,
      style: `font-stretch: ${key}`,
    })),
  );
};

/**
 * Creates a style filter as toggle buttons
 */
const StyleFilter = (styleFilter: State<string[]>) => {
  return FilterGroup(
    styleFilter,
    "Style",
    Object.entries(FONT_STYLE_CATEGORIES).map(([key, category]) => ({
      key,
      label: category.label,
      style: `font-style: ${key}`,
    })),
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

const ToggleButton =
  (body: ChildDom, title: string, onclick: () => void, active: State<boolean>) => () => {
    return button(
      {
        class: active.val ? "toggle-btn active" : "toggle-btn",
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
  return div({ class: "text-sm" }, text);
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
      { class: "card flex flex-col gap-sm", style: "margin-bottom: 1rem" },
      SearchInput(filterStates.searchQuery),
      div(
        { class: "flex flex-col gap-xs" },
        WeightFilter(filterStates.weightFilter, showNumber),
        StretchFilter(filterStates.stretchFilter),
        StyleFilter(filterStates.styleFilter),
      ),
      ClearFiltersButton(clearFilters),
      div({ class: "divider" }),
      div(
        { class: "flex flex-wrap justify-between items-center gap-sm" },
        StatsText(stats),
        ToggleButton(
          "Show Numbers",
          "Toggle to show weight and stretch numbers",
          () => {
            showNumber.val = !showNumber.val;
          },
          showNumber,
        ),
      ),
    );
  };
