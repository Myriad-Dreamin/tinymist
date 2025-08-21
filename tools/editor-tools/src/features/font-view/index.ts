import van, { type State } from "vanjs-core";
import { base64Decode } from "../../utils";
import { type StyleAtCursor, styleAtCursor } from "../../vscode";
import {
  ClearFiltersButton,
  SearchInput,
  StretchFilter,
  StyleFilter,
  WeightFilter,
} from "./components";
import { filterFontFamilies } from "./filtering";
import { FontAction, FontFamilySlot } from "./font-components";
import { DOC_MOCK } from "./mock-data";
import type { FontFamily, FontResources } from "./types";
import "./styles.css";

const { div } = van.tags;

/**
 * Creates filter state management utilities
 */
function createFilterStates() {
  return {
    searchQuery: van.state(""),
    weightFilter: van.state(""),
    styleFilter: van.state(""),
    stretchFilter: van.state(""),
  };
}

/**
 * Creates a clear all filters function
 */
function createClearFilters(filterStates: ReturnType<typeof createFilterStates>) {
  return () => {
    filterStates.searchQuery.val = "";
    filterStates.weightFilter.val = "";
    filterStates.styleFilter.val = "";
    filterStates.stretchFilter.val = "";
  };
}

/**
 * Creates the font statistics display
 */
function createFontStats(
  fontResources: State<FontResources>,
  filteredFamilies: State<FontFamily[]>,
) {
  return van.derive(() => {
    const total = fontResources.val.families.length;
    const filtered = filteredFamilies.val.length;
    const variants = filteredFamilies.val.reduce(
      (sum: number, family: FontFamily) => sum + family.infos.length,
      0,
    );

    if (filtered === total) {
      return `Showing ${total} font families (${variants} variants)`;
    } else {
      return `Showing ${filtered} of ${total} font families (${variants} variants)`;
    }
  });
}

/**
 * Creates the font families container content
 */
function createFontFamiliesContent(
  filteredFamilies: State<FontFamily[]>,
  fontResources: State<FontResources>,
  showNumberOpt: State<{ showNumber: boolean }>,
) {
  return (_dom?: Element) =>
    div(
      { class: "font-families-container" },
      filteredFamilies.val.length === 0
        ? div({ class: "no-fonts-message" }, "No fonts match the current filters")
        : filteredFamilies.val.map((family: FontFamily) =>
            FontFamilySlot(family, fontResources.val, showNumberOpt.val),
          ),
    );
}

/**
 * Initializes font resources data from various sources
 */
function initializeFontResources(): State<FontResources> {
  const FontResourcesData = `:[[preview:FontInformation]]:`;
  return van.state<FontResources>(
    FontResourcesData.startsWith(":") ? DOC_MOCK : JSON.parse(base64Decode(FontResourcesData)),
  );
}

/**
 * Initializes style at cursor data
 */
function initializeStyleAtCursor(): State<StyleAtCursor | undefined> {
  const StyleAtCursorData = `:[[preview:StyleAtCursor]]:`;
  return van.state<StyleAtCursor | undefined>(
    StyleAtCursorData.startsWith(":") ? undefined : JSON.parse(base64Decode(StyleAtCursorData)),
  );
}

/**
 * Sets up reactive style at cursor updates
 */
function setupStyleAtCursorReactivity(lastStylesAtCursor: State<StyleAtCursor | undefined>) {
  van.derive(() => {
    const version = styleAtCursor.val?.version;
    const lastVersion = lastStylesAtCursor.val?.version;
    if (version && (typeof lastVersion !== "number" || lastVersion < version)) {
      lastStylesAtCursor.val = styleAtCursor.val;
    }
  });
}

/**
 * Main Font View Component
 * Provides a comprehensive interface for browsing and interacting with fonts
 */
export const FontView = () => {
  // Initialize data sources
  const fontResources = initializeFontResources();
  const lastStylesAtCursor = initializeStyleAtCursor();

  // Setup reactivity
  setupStyleAtCursorReactivity(lastStylesAtCursor);

  // State management
  const showNumber = van.state(false);
  const showNumberOpt = van.derive(() => ({ showNumber: showNumber.val }));

  // Filter states
  const filterStates = createFilterStates();
  const clearFilters = createClearFilters(filterStates);

  // Filtered families computation
  const filteredFamilies = van.derive(() => {
    try {
      return filterFontFamilies(fontResources.val, {
        searchQuery: filterStates.searchQuery.val,
        weightFilter: filterStates.weightFilter.val,
        styleFilter: filterStates.styleFilter.val,
        stretchFilter: filterStates.stretchFilter.val,
      });
    } catch (error) {
      console.error("Error filtering font families:", error);
      return [];
    }
  });

  const fontStats = createFontStats(fontResources, filteredFamilies);
  const fontFamiliesContent = createFontFamiliesContent(
    filteredFamilies,
    fontResources,
    showNumberOpt,
  );

  return div(
    { class: "font-view-container" },
    div(
      { class: "font-view-header flex-col" },
      div(
        { class: "font-view-controls" },
        SearchInput(filterStates.searchQuery),
        div(
          { class: "font-filter-group" },
          WeightFilter(filterStates.weightFilter),
          StretchFilter(filterStates.stretchFilter),
          StyleFilter(filterStates.styleFilter),
          FontAction(
            "Show Numbers",
            "Toggle to show weight and stretch numbers",
            () => {
              showNumber.val = !showNumber.val;
            },
            { active: showNumber, variant: "toggle" },
          ),
          ClearFiltersButton(clearFilters),
        ),
      ),
      div({ class: "font-stats" }, fontStats),
    ),
    fontFamiliesContent,
  );
};
