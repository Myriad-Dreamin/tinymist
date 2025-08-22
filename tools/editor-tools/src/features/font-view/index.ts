import van, { type State } from "vanjs-core";
import { base64Decode } from "@/utils";
import { type StyleAtCursor, styleAtCursor } from "@/vscode";
import { useFontFilters } from "./filtering";
import type { FontResources } from "./fonts";
import { MOCK_DATA } from "./mock-data";
import "./styles.css";
import { FontList } from "./components/font-list";
import { Header } from "./components/header";

const { div } = van.tags;

/**
 * Initializes font resources data from various sources
 */
function initializeFontResources(): State<FontResources> {
  const FontResourcesData = `:[[preview:FontInformation]]:`;
  return van.state<FontResources>(
    FontResourcesData.startsWith(":") ? MOCK_DATA : JSON.parse(base64Decode(FontResourcesData)),
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
 */
export const FontView = () => {
  // Initialize data sources
  const fontResources = initializeFontResources();
  const lastStylesAtCursor = initializeStyleAtCursor();

  // Setup reactivity
  setupStyleAtCursorReactivity(lastStylesAtCursor);

  const showNumber = van.state(false);
  const { fontFilters, clearFilters, filteredFamilies, fontStats } = useFontFilters(fontResources);

  return div(
    { class: "font-view-container text-base-content" },
    Header(fontFilters, fontStats, showNumber, clearFilters),
    FontList(filteredFamilies, fontResources, showNumber),
  );
};
