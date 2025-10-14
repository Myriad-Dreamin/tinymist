import type { ExportFormat, PreviewData, PreviewPage, Scalar } from "./types";
import { requestExportDocument, requestGeneratePreview } from "@/vscode";
import van, { State } from "vanjs-core";
import { EXPORT_FORMATS } from "./config/formats";

type PreviewResponse = PreviewData & { type: string; version: number };

export function useExporter() {
  const inputPath = van.state("");
  const outputPath = van.state("");
  const format = van.state(EXPORT_FORMATS[0]);
  const optionStates: Record<string, State<Scalar>> = {};

  let previewVersion = 0;
  const previewGenerating = van.state(false);
  const previewData = van.state<PreviewData>({});
  const error = van.state<string | undefined>(undefined);

  const buildOptions = () =>
    Object.fromEntries(
      format.val.options.map((option) => [option.key, optionStates[option.key]?.val]),
    );

  const exportDocument = () => {
    const exportOptions = buildOptions();
    console.log("Exporting document as", format.val.id, "With options:", exportOptions);
    requestExportDocument(format.val.id, exportOptions);
  };

  const generatePreview = () => {
    previewGenerating.val = true;
    const exportOptions = buildOptions();
    console.log("Generate preview as", format.val.id, "With options:", exportOptions);
    requestGeneratePreview(format.val.id, { ...exportOptions }, ++previewVersion);

    if (import.meta.env.DEV) {
      // Simulate preview generation in dev mode
      window.postMessage(createMockPreviewResponse(format.val, previewVersion));
    }
  };

  // Regenerate preview when format changes
  van.derive(() => {
    format.val; // Track format changes
    previewData.val = {};
    generatePreview();
  });

  const handleMessage = (event: MessageEvent) => {
    console.log("Received message event", event);
    const data = event.data;
    if (data.type === "previewGenerated") {
      if (data.version < previewVersion) {
        console.log("Ignoring outdated preview version", data.version, "<", previewVersion);
        return;
      }
      previewData.val = data;
      error.val = data.error;
      previewGenerating.val = false;
    } else if (data.type === "previewError") {
      if (data.version < previewVersion) {
        return;
      }
      error.val = data.error;
      previewGenerating.val = false;
    }
  };
  window?.addEventListener("message", handleMessage);

  return {
    inputPath,
    outputPath,
    format,
    optionStates,
    previewGenerating,
    previewData,
    exportDocument,
    generatePreview,
  };
}

function createMockPreviewResponse(format: ExportFormat, version: number): PreviewResponse {
  if (format.id === "pdf" || format.id === "png" || format.id === "svg") {
    const MOCK_IMAGE =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    const pages: PreviewPage[] = [
      { pageNumber: 1, imageData: MOCK_IMAGE },
      { pageNumber: 2, imageData: MOCK_IMAGE },
    ];

    return {
      type: "previewGenerated",
      version,
      pages,
    };
  }

  return {
    type: "previewGenerated",
    version,
    text: `# ${format.label} Preview\n\nThis is mock preview data for ${format.label}.\n\n- Item 1\n- Item 2\n- Item 3`,
  };
}
