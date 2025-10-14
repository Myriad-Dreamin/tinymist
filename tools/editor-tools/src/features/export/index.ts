import van from "vanjs-core";
import "./styles.css";

import { ActionButtons } from "./components/action-buttons";
import { FormatSelector } from "./components/format-selector";
import { Header } from "./components/header";
import { DocumentUriSection, OptionsPanel } from "./components/options-panel";
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
    exportDocument,
    generatePreview,
  } = useExporter();

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
    () => OptionsPanel({ format: format.val, optionStates }),

    // Preview Section
    () =>
      PreviewGrid({
        format: format.val,
        previewData,
        previewGenerating,
        onPreview: generatePreview,
      }),

    // Export Actions
    ActionButtons({
      onExport: exportDocument,
    }),
  );
};

export default ExportTool;
