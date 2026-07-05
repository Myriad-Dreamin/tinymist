import van, { type State } from "vanjs-core";
import type { ExportFormat, PreviewData, PreviewPage } from "../types";

const { div, h3, img, span, button, label, input } = van.tags;

interface PreviewGridProps {
  format: State<ExportFormat>;
  previewData: State<PreviewData>;
  previewGenerating: State<boolean>;
  autoPreview: State<boolean>;
  onPreview: () => void;
}

export const PreviewGrid = (props: PreviewGridProps) => {
  const { format, previewData, previewGenerating, autoPreview, onPreview } = props;

  const thumbnailZoom = van.state<number>(100); // Percentage for thumbnail sizing

  return div(
    // Preview Header
    div(
      { class: "flex justify-between items-center mb-md" },
      div(
        { class: "flex items-center gap-sm" },

        h3({ class: "text-lg font-semibold" }, () => `Preview (${format.val.label})`),

        // Generate Preview Button
        () =>
          button(
            {
              class: "btn btn-secondary",
              onclick: onPreview,
              disabled: previewGenerating.val,
            },
            previewGenerating.val ? "Generating..." : "Generate Preview",
          ),

        // Auto-preview toggle
        label(
          { class: "flex items-center gap-xs cursor-pointer select-none" },
          input({
            type: "checkbox",
            class: "toggle",
            checked: autoPreview,
            onchange: (e: Event) => {
              const target = e.target as HTMLInputElement;
              autoPreview.val = target.checked;
            },
          }),
          span({ class: "text-sm" }, "Auto"),
        ),
      ),

      () =>
        div(
          { class: "flex items-center gap-sm", style: "min-height: 2rem;" },

          // Only show zoom controls for image content (thumbnails)
          !previewGenerating.val && previewData.val.pages && previewData.val.pages.length > 0
            ? ZoomControls(thumbnailZoom)
            : null,
        ),
    ),

    // Preview Content
    () =>
      (() => {
        if (previewData.val.error) {
          return PreviewError(previewData.val.error);
        }

        // Handle text content for text-based formats
        if (previewData.val.text) {
          return PreviewTextContent(previewData.val.text);
        }

        // Handle image pages for visual formats
        if (previewData.val.pages) {
          return PreviewPagesGrid(previewData.val.pages, thumbnailZoom.val);
        }

        if (previewGenerating.val) {
          return PreviewLoading();
        }

        return PreviewEmpty(onPreview);
      })(),
  );
};

const PreviewLoading = () => {
  return div(
    { class: "preview-loading" },
    div({ class: "action-spinner" }),
    "Generating preview...",
  );
};

const PreviewError = (errorMessage: string) => {
  return div(
    { class: "preview-error" },
    span("⚠️ Failed to generate preview"),
    span({ class: "text-sm" }, errorMessage),
  );
};

const PreviewEmpty = (onGenerate: () => void) => {
  return div(
    { class: "preview-loading" },
    div(
      { class: "text-center" },
      div({ class: "mb-md" }, "No preview available"),
      button(
        {
          class: "btn",
          onclick: onGenerate,
        },
        "Flush",
      ),
    ),
  );
};

const PreviewPagesGrid = (
  pages: PreviewPage[],
  thumbnailZoom: number,
  onImageClick?: (page: PreviewPage) => void,
) => {
  const baseSize = 200; // Base thumbnail size
  const scaledSize = Math.round(baseSize * (thumbnailZoom / 100));

  return div(
    {
      class: "preview-grid",
      style: `--thumbnail-size: ${scaledSize}px;`,
    },
    ...pages.map((page) => PreviewPageCard(page, onImageClick)),
  );
};

const PreviewPageCard = (page: PreviewPage, onImageClick?: (page: PreviewPage) => void) => {
  return div(
    {
      class: "preview-page",
      onclick: () => onImageClick?.(page),
    },
    img({
      class: "preview-page-image",
      src: page.imageData,
      alt: `Page ${page.pageNumber + 1}`,
      loading: "lazy",
    }),
    span(
      {
        class: "preview-page-number",
      },
      `${page.pageNumber + 1}`,
    ),
  );
};

const PreviewTextContent = (text: string) => {
  return div(
    { class: "preview-text-container" },
    div({ class: "preview-text-content" }, text ?? "No text content available"),
  );
};

const ZoomControls = (zoom: State<number>) => {
  const MAX_ZOOM = 300;
  const MIN_ZOOM = 25;

  const adjustZoom = (delta: number) => {
    const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom.val + delta));
    zoom.val = newZoom;
  };

  return div(
    { class: "zoom-control flex items-center gap-xs" },
    span({ class: "zoom-label" }, "Thumbnails:"),
    button(
      {
        class: "btn btn-secondary",
        onclick: () => adjustZoom(-25),
        disabled: zoom.val <= MIN_ZOOM,
        title: "Smaller thumbnails",
      },
      "−",
    ),
    span({ class: "text-xs font-medium", style: "width: 3em" }, () => `${zoom.val}%`),
    button(
      {
        class: "btn btn-secondary",
        onclick: () => adjustZoom(25),
        disabled: zoom.val >= MAX_ZOOM,
        title: "Larger thumbnails",
      },
      "+",
    ),
    button(
      {
        class: "btn btn-secondary",
        onclick: () => {
          zoom.val = 100;
        },
        title: "Reset thumbnail size",
      },
      "100%",
    ),
  );
};
