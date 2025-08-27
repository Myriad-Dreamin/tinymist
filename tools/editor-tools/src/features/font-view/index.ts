import van, { type State } from "vanjs-core";
import { base64Decode } from "@/utils";
import { useFontFilters } from "./filtering";
import type { FontResources } from "./fonts";
import "./styles.css";
import { FontList } from "./components/font-list";
import { Header } from "./components/header";

const { div } = van.tags;

function useFontResources(): State<FontResources> {
  const stub = `:[[preview:FontInformation]]:`;

  const fontResources = van.state<FontResources>(
    stub.startsWith(":") ? { sources: [], families: [] } : JSON.parse(base64Decode(stub)),
  );

  if (import.meta.env.DEV) {
    // Dynamically import mock data in development mode if no real data is present
    import("./mock-data").then((module) => {
      fontResources.val = module.MOCK_DATA;
    });
  }

  return fontResources;
}

/**
 * Main Font View Component
 */
const FontView = () => {
  // Initialize data sources
  const fontResources = useFontResources();
  console.log("fontResources", fontResources.val);

  const showNumber = van.state(false);
  const { fontFilters, clearFilters, filteredFamilies, fontStats } = useFontFilters(fontResources);

  return div(
    { class: "font-view-container text-base-content" },
    Header(fontFilters, fontStats, showNumber, clearFilters),
    FontList(filteredFamilies, fontResources, showNumber),
  );
};

export default FontView;
