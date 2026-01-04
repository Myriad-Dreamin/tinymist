import van, { type State } from "vanjs-core";
import { lastFocusedTypstDoc, requestExportDocument, requestGeneratePreview } from "@/vscode";
import { EXPORT_FORMATS } from "./formats";
import type { ExportFormat, PreviewData, PreviewPage, Scalar } from "./types";

type PreviewResponse = PreviewData & { type: string; version: number };

export function useExporter() {
  const inputPath = van.state("");
  const outputPath = van.state("");
  const format = van.state(EXPORT_FORMATS[0]);
  // option value may be scalar or an array (multi-select)
  const optionStates: Record<string, State<Scalar | Scalar[] | undefined>> = {};
  const compilerInputs = van.state<[string, string][]>([]);

  let previewVersion = 0;
  const previewGenerating = van.state(false);
  const previewData = van.state<PreviewData>({});
  const autoPreview = van.state(true);

  const buildOptions = () => {
    const extraOpts = Object.fromEntries(
      format.val.options.map((option) => {
        const val = optionStates[option.key]?.val;
        // treat empty strings or empty arrays as undefined so the server can pick defaults
        if (val === "") return [option.key, undefined];
        if (Array.isArray(val) && val.length === 0) return [option.key, undefined];
        return [option.key, val];
      }),
    );
    return {
      inputPath: inputPath.val.length > 0 ? inputPath.val : lastFocusedTypstDoc.val,
      outputPath: outputPath.rawVal.length > 0 ? outputPath.rawVal : undefined,
      inputs: compilerInputs.val,
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
    if (!autoPreview.val) return;

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

  const cleanup = () => {
    window?.removeEventListener("message", handleMessage);
  };

  return {
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
    cleanup,
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

    const pages: PreviewPage[] = [];
    for (let i = 0; i < 100; i++) {
      pages.push({ pageNumber: i, imageData: MOCK_IMAGE });
    }

    return {
      type: "previewGenerated",
      version,
      pages,
    };
  }

  const text = `# ${format.label} Preview\n\nThis is mock preview data for ${format.label}.\n\n- Item 1\n- Item 2\n- Item 3`;
  return {
    type: "previewGenerated",
    version,
    text: text.repeat(10),
  };
}
