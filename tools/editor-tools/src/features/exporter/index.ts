import van from "vanjs-core";
import "./styles.css";

import { ActionButtons } from "./components/action-buttons";
import { FormatSelector } from "./components/format-selector";
import { InputSection } from "./components/inout";
import { OptionsPanel } from "./components/options-panel";
import { PreviewGrid } from "./components/preview-grid";
import { useExporter } from "./exporter";

const { div } = van.tags;

/**
 * Main Export Tool Component
 */
const ExportTool = () => {
  // Initialize state
  const {
    inputPath,
    outputPath,
    format,
    optionStates,
    previewGenerating,
    previewData,
    autoPreview,
    exportDocument,
    generatePreview,
  } = useExporter();

  // Note: cleanup() should be called when the component is unmounted
  // In the current single-page architecture, this might not be needed,
  // but it's available for future use if the tool becomes part of a larger app

  return div(
    { class: "export-tool-container flex flex-col gap-lg text-base-content" },

    // Input Document Section
    InputSection({ inputPath, outputPath }),

    // Export Actions
    ActionButtons({
      onExport: exportDocument,
    }),

    // Format Selection
    FormatSelector({ selectedFormat: format }),

    // Options Configuration
    () => OptionsPanel({ format: format.val, optionStates }),

    // Preview Section
    PreviewGrid({
      format: format,
      previewData,
      previewGenerating,
      autoPreview,
      onPreview: generatePreview,
    }),
  );
};

export default ExportTool;
