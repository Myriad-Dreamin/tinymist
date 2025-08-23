import "./styles.css";
import van from "vanjs-core";
import { base64Decode } from "@/utils";
import { useCategorizedSymbols } from "./categorized";
import { CanvasPanel } from "./components/canvas-panel";
import { SymbolPicker } from "./components/symbol-picker";
import { SearchBar, ViewModeToggle } from "./components/toolbox";
import { useDetypifyFilter } from "./detypify-filter";
import { prerenderSymbols } from "./render";
import { useSymbolSearch } from "./search-filter";
import type { SymbolItem } from "./symbols";

const { div } = van.tags;

function useSymbolResource() {
  // Get symbol information from the embedded data
  const symbolInformationData = `:[[preview:SymbolInformation]]:`;
  const symbols = van.state<SymbolItem[]>(
    symbolInformationData.startsWith(":")
      ? []
      : prerenderSymbols(JSON.parse(base64Decode(symbolInformationData))),
  );

  if (import.meta.env.DEV) {
    // Dynamically import mock data in development mode if no real data is present
    import("./mock-data").then((module) => {
      symbols.val = prerenderSymbols(module.default);
      console.log("symbols", symbols.val);
    });
  }

  console.log("symbols", symbols.val);

  return symbols;
}

export const SymbolView = () => {
  const symbols = useSymbolResource();

  const { strokes: detypifyStrokes, drawCandidates } = useDetypifyFilter();
  const { filteredSymbols, updateFilter } = useSymbolSearch(symbols);
  const { categorizedSymbols } = useCategorizedSymbols(symbols, filteredSymbols, drawCandidates);

  const showSymbolDetails = van.state(false);

  return div(
    { class: "tinymist-symbol-view flex gap-md text-base-content" },
    div(
      { class: "symbol-toolbox card flex flex-col items-center gap-sm" },
      SearchBar(updateFilter),
      CanvasPanel(detypifyStrokes),
      ViewModeToggle(showSymbolDetails),
    ),
    SymbolPicker(categorizedSymbols, showSymbolDetails),
  );
};
