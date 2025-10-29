import van, { type State } from "vanjs-core";
import { lastFocusedTypstDoc, requestExportDocument, requestGeneratePreview } from "@/vscode";
import { EXPORT_FORMATS } from "./formats";
import type { ExportFormat, PreviewData, PreviewPage, Scalar } from "./types";

type PreviewResponse = PreviewData & { type: string; version: number };

export function useExporter() {
  const inputPath = van.state("");
  const outputPath = van.state("");
  const format = van.state(EXPORT_FORMATS[0]);
  const optionStates: Record<string, State<Scalar>> = {};

  let previewVersion = 0;
  const previewGenerating = van.state(false);
  const previewData = van.state<PreviewData>({});

  const buildOptions = () => {
    const extraOpts = Object.fromEntries(
      format.val.options.map((option) => {
        const val = optionStates[option.key]?.val;
        return [option.key, val === "" ? undefined : val];
      }),
    );
    return {
      inputPath: inputPath.val.length > 0 ? inputPath.val : lastFocusedTypstDoc.val,
      outputPath: outputPath.rawVal.length > 0 ? outputPath.rawVal : undefined,
      ...extraOpts,
    };
  };

  const exportDocument = () => {
    const exportOptions = buildOptions();
    console.log("Exporting document as", format.val.id, "With options:", exportOptions);
    requestExportDocument(format.val.id, exportOptions);
  };

  const generatePreview = () => {
    previewGenerating.val = true;
    const exportOptions = buildOptions();
    console.log("Generate preview as", format.val.id, "With options:", exportOptions);
    requestGeneratePreview(format.val.id, exportOptions, ++previewVersion);

    if (import.meta.env.DEV) {
      // Simulate preview generation in dev mode
      setTimeout(
        () => window.postMessage(createMockPreviewResponse(format.val, previewVersion)),
        Math.random() * 100 + 100,
      );
    }
  };

  // Regenerate preview automatically when format or options change
  van.derive(() => {
    if (format.oldVal !== format.val) {
      // Clear previous preview data when format changes
      previewData.val = {};
    }
    generatePreview();
  });

  const handleMessage = (event: MessageEvent) => {
    const data = event.data;
    if (data.type === "previewGenerated" || data.type === "previewError") {
      if (data.version < previewVersion) {
        return;
      }
      previewData.val = data;
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
  if (Math.random() < 0.1) {
    return {
      type: "previewError",
      version,
      error: "Mock preview generation error",
    };
  }

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
