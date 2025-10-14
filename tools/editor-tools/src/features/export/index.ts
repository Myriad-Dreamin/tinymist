import van, { type State } from "vanjs-core";
import { MOCK_PREVIEW_PAGES } from "./mock-data";
import type { ExportConfig, ExportConfigState, OptionSchema, PreviewPage, Scalar } from "./types";
import "./styles.css";

import { ActionButtons } from "./components/action-buttons";
import { FormatSelector } from "./components/format-selector";
// Components
import { Header } from "./components/header";
import { DocumentUriSection, OptionsPanel } from "./components/options-panel";
import { PreviewGrid } from "./components/preview-grid";
import { EXPORT_FORMATS, getDefaultOptions } from "./config/formats";

const { div } = van.tags;

function useExportConfig(): State<ExportConfig> {
  return van.state<ExportConfig>({
    format: EXPORT_FORMATS[0], // PDF format
    inputPath: "",
    outputPath: "",
    options: getDefaultOptions(EXPORT_FORMATS[0]),
  });
}

function usePreviewPages(): State<PreviewPage[]> {
  const previewPages = van.state<PreviewPage[]>(MOCK_PREVIEW_PAGES);

  return previewPages;
}

/**
 * Main Export Tool Component
 */
const ExportTool = () => {
  // Initialize state
  // const exportConfig = useExportConfig();
  const inputPath = van.state("");
  const outputPath = van.state("");
  const format = van.state(EXPORT_FORMATS[0]);
  const optionStates: Record<string, State<Scalar>> = {};

  const previewPages = usePreviewPages();

  for (const format of EXPORT_FORMATS) {
    for (const option of format.options) {
      if (!optionStates[option.key]) {
        optionStates[option.key] = van.state(option.default);
      }
    }
  }

  console.log("initial option states", optionStates);

  const activeOptions = van.derive(() => {
    console.log("Deriving activeOptions for format", format.val.id);
    const formatOptions = format.val.options;
    for (const option of formatOptions) {
      if (!optionStates[option.key]) {
        optionStates[option.key] = van.state(option.default);
      }
    }

    return formatOptions;
  });

  van.derive(() => console.log("Active changed", activeOptions.val));

  return div(
    { class: "export-tool-container flex flex-col gap-lg text-base-content" },
    Header({
      title: "Export Tool",
      description: "Configure and export your Typst documents to various formats",
    }),

    // Input Document Section
    DocumentUriSection(),

    // Format Selection
    FormatSelector({ selectedFormat: format }),

    // Options Configuration
    () =>
      OptionsPanel({
        format: format.val,
        options: activeOptions.val,
        optionStates,
      }),

    // Preview Section
    // exportConfig.val.format.supportsPreview ? PreviewGrid({ exportConfig, previewPages }) : null,

    // Export Actions
    // ActionButtons({ exportConfig }),
  );
};

export default ExportTool;
