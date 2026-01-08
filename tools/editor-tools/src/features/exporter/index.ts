import van from "vanjs-core";
import "./styles.css";

import { CompilerInputs } from "./components/compiler-inputs";
import { FormatSelector } from "./components/format-selector";
import { InputSection } from "./components/inout";
import { OptionsPanel } from "./components/options-panel";
import { PreviewGrid } from "./components/preview-grid";
import { useExporter } from "./exporter";

const { div, button } = van.tags;

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
    compilerInputs,
    previewGenerating,
    previewData,
    autoPreview,
    exportDocument,
    generatePreview,
  } = useExporter();

  // Note: cleanup() should be called when the component is unmounted
  // In the current single-page architecture, this might not be needed,
  // but it's available for future use if the tool becomes part of a larger app

  const exportBtn = button(
    {
      title: "Immediately export the current document with these settings",
      class: "btn action-button",
      onclick: exportDocument,
    },
    "Export",
  );

  return div(
    { class: "export-tool-container flex flex-col gap-lg text-base-content" },

    // Input Document Section
    InputSection({ inputPath, outputPath, actionButton: exportBtn }),

    // Compiler Inputs Section
    CompilerInputs({ inputs: compilerInputs }),

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
