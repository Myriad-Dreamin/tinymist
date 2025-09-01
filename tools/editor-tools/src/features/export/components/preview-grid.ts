import van, { type State } from "vanjs-core";
import type { ExportConfig, PreviewPage } from "../types";
import { requestGeneratePreview } from "../../../vscode";

const { div, h3, img, span, button } = van.tags;

interface PreviewGridProps {
  exportConfig: State<ExportConfig>;
  previewPages: State<PreviewPage[]>;
}

export const PreviewGrid = ({ exportConfig, previewPages }: PreviewGridProps) => () => {
  const { format } = exportConfig.val;
  const isLoading = van.state<boolean>(false);
  const error = van.state<string | null>(null);
  const zoomLevel = van.state<number>(100); // Percentage
  const textContent = van.state<string | null>(null); // For text-based formats

  // Don't show preview for non-visual formats
  if (!format.supportsPreview) {
    return div();
  }

  const generatePreview = async () => {
    isLoading.val = true;
    error.val = null;
    textContent.val = null; // Clear previous text content

    try {
      // Build export args from options
      const { options } = exportConfig.val;

      // Request preview generation from VSCode extension
      requestGeneratePreview(format.id, options);

      // Response will be handled via VSCode channel in setupPreviewListeners
    } catch (err) {
      error.val = err instanceof Error ? err.message : "Failed to generate preview";
      isLoading.val = false;
    }
  };

  // Set up listeners for preview responses
  const setupPreviewListeners = () => {
    const handleMessage = (event: MessageEvent) => {
      if (event.data.type === "previewGenerated") {
        if (event.data.format === format.id) {
          // Handle both image pages and text content
          if (event.data.pages) {
            // Visual format with image pages
            previewPages.val = event.data.pages || [];
            textContent.val = null;
          } else if (event.data.text) {
            // Text-based format
            textContent.val = event.data.text;
            previewPages.val = [];
          }
          isLoading.val = false;
        }
      } else if (event.data.type === "previewError") {
        error.val = event.data.error;
        isLoading.val = false;
      }
    };

    // Add event listener when component is rendered
    if (typeof window !== 'undefined') {
      window.addEventListener("message", handleMessage);

      // Cleanup function would be called when component is destroyed
      // In VanJS, we can use the reactive pattern for cleanup
    }
  };

  // Set up listeners when component is first created
  van.derive(() => {
    setupPreviewListeners();
  });

  const adjustZoom = (delta: number) => {
    const newZoom = Math.max(25, Math.min(400, zoomLevel.val + delta));
    zoomLevel.val = newZoom;
  };

  return div(
    { class: "preview-section" },

    // Preview Header
    div(
      { class: "preview-header" },
      h3({ class: "preview-title" }, `Preview (${format.label})`),
      div(
        { class: "preview-controls" },
        button(
          {
            class: "btn btn-secondary",
            onclick: generatePreview,
            disabled: isLoading.val
          },
          isLoading.val ? "Generating..." : "Generate Preview"
        ),
        div(
          { class: "zoom-control" },
          button(
            {
              class: "zoom-button",
              onclick: () => adjustZoom(-25),
              disabled: zoomLevel.val <= 25
            },
            "−"
          ),
          span({ class: "zoom-label" }, () => `${zoomLevel.val}%`),
          button(
            {
              class: "zoom-button",
              onclick: () => adjustZoom(25),
              disabled: zoomLevel.val >= 400
            },
            "+"
          )
        )
      )
    ),

    // Preview Content
    (() => {
      if (isLoading.val) {
        return PreviewLoading();
      }

      if (error.val) {
        return PreviewError(error.val, generatePreview);
      }

      // Handle text content for text-based formats
      if (textContent.val) {
        return PreviewTextContent(textContent.val, zoomLevel.val);
      }

      // Handle image pages for visual formats
      if (previewPages.val.length === 0) {
        return PreviewEmpty(generatePreview);
      }

      return PreviewPagesGrid(previewPages.val, zoomLevel.val);
    })()
  );
};

const PreviewLoading = () => {
  return div(
    { class: "preview-loading" },
    div({ class: "action-spinner" }),
    "Generating preview..."
  );
};

const PreviewError = (errorMessage: string, onRetry: () => void) => {
  return div(
    { class: "preview-error" },
    span("⚠️ Failed to generate preview"),
    span({ style: "font-size: 0.75rem;" }, errorMessage),
    button(
      {
        class: "btn btn-secondary",
        style: "margin-top: 0.5rem;",
        onclick: onRetry
      },
      "Retry"
    )
  );
};

const PreviewEmpty = (onGenerate: () => void) => {
  return div(
    { class: "preview-loading" },
    div(
      { style: "text-align: center;" },
      span({ style: "display: block; margin-bottom: 1rem;" }, "No preview available"),
      button(
        {
          class: "btn",
          onclick: onGenerate
        },
        "Generate Preview"
      )
    )
  );
};

const PreviewPagesGrid = (pages: PreviewPage[], zoom: number) => {
  const scaleStyle = `transform: scale(${zoom / 100}); transform-origin: top left;`;

  return div(
    { class: "preview-grid" },
    ...pages.map(page => PreviewPageCard(page, scaleStyle))
  );
};

const PreviewPageCard = (page: PreviewPage, scaleStyle: string) => {
  return div(
    {
      class: "preview-page",
      style: scaleStyle,
      title: `Page ${page.pageNumber} (${page.width}×${page.height})`
    },
    img({
      class: "preview-page-image",
      src: page.imageData,
      alt: `Page ${page.pageNumber}`,
      loading: "lazy"
    }),
    span({ class: "preview-page-number" }, page.pageNumber.toString())
  );
};

const PreviewTextContent = (text: string, zoom: number) => {
  const scaleStyle = `transform: scale(${zoom / 100}); transform-origin: top left;`;

  return div(
    { class: "preview-text-container" },
    div(
      {
        class: "preview-text-content",
        style: `${scaleStyle} max-height: 400px; overflow-y: auto; border: 1px solid #ddd; padding: 1rem; background: #f9f9f9; font-family: monospace; white-space: pre-wrap;`
      },
      text
    )
  );
};
