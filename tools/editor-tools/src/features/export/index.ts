import van, { type State } from "vanjs-core";
import { base64Decode } from "../../utils";
import type { ExportConfig, PreviewPage } from "./types";
import { MOCK_EXPORT_CONFIG, MOCK_PREVIEW_PAGES } from "./mock-data";
import "./styles.css";

// Components
import { Header } from "./components/header";
import { FormatSelector } from "./components/format-selector";
import { OptionsPanel } from "./components/options-panel";
import { PreviewGrid } from "./components/preview-grid";
import { ActionButtons } from "./components/action-buttons";

const { div } = van.tags;

function useExportConfig(): State<ExportConfig> {
  const stub = `:[[preview:ExportConfig]]:`;

  const exportConfig = van.state<ExportConfig>(
    stub.startsWith(":") ?
      MOCK_EXPORT_CONFIG :
      JSON.parse(base64Decode(stub))
  );

  return exportConfig;
}

function usePreviewPages(): State<PreviewPage[]> {
  const stub = `:[[preview:PreviewPages]]:`;

  const previewPages = van.state<PreviewPage[]>(
    stub.startsWith(":") ?
      MOCK_PREVIEW_PAGES :
      JSON.parse(base64Decode(stub))
  );

  return previewPages;
}

/**
 * Main Export Tool Component
 */
const ExportTool = () => {
  // Initialize state
  const exportConfig = useExportConfig();
  const previewPages = usePreviewPages();

  console.log("Export config:", exportConfig.val);
  console.log("Preview pages:", previewPages.val);

  return div(
    { class: "export-tool-container text-base-content" },
    Header({
      title: "Export Tool",
      description: "Configure and export your Typst documents to various formats"
    }),

    // Format Selection
    div(
      { style: "margin-bottom: 1.5rem;" },
      div(
        {
          style: "margin-bottom: 1rem; font-size: 1.125rem; font-weight: 600;"
        },
        "1. Choose Export Format"
      ),
      FormatSelector({ exportConfig })
    ),

    // Options Configuration
    div(
      { style: "margin-bottom: 1.5rem;" },
      div(
        {
          style: "margin-bottom: 1rem; font-size: 1.125rem; font-weight: 600;"
        },
        "2. Configure Options"
      ),
      OptionsPanel({ exportConfig })
    ),

    // Preview Section
    exportConfig.val.format.supportsPreview ? div(
      { style: "margin-bottom: 1.5rem;" },
      div(
        {
          style: "margin-bottom: 1rem; font-size: 1.125rem; font-weight: 600;"
        },
        "3. Preview"
      ),
      PreviewGrid({ exportConfig, previewPages })
    ) : null,

    // Export Actions
    div(
      { style: "margin-bottom: 1.5rem;" },
      div(
        {
          style: "margin-bottom: 1rem; font-size: 1.125rem; font-weight: 600;"
        },
        exportConfig.val.format.supportsPreview ? "4. Export Actions" : "3. Export Actions"
      ),
      ActionButtons({ exportConfig })
    )
  );
};

export default ExportTool;
