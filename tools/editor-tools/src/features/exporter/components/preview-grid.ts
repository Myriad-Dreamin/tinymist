import van, { type State } from "vanjs-core";
import type { ExportFormat, PreviewData, PreviewPage } from "../types";

const { div, h3, img, span, button } = van.tags;

interface PreviewGridProps {
  format: ExportFormat;
  previewData: State<PreviewData>;
  previewGenerating: State<boolean>;
  onPreview: () => void;
}

export const PreviewGrid = (props: PreviewGridProps) => {
  const { format, previewData, previewGenerating, onPreview } = props;

  const thumbnailZoom = van.state<number>(100); // Percentage for thumbnail sizing
  const selectedImage = van.state<PreviewPage | null>(null); // For image modal

  const adjustThumbnailZoom = (delta: number) => {
    const newZoom = Math.max(50, Math.min(300, thumbnailZoom.val + delta));
    thumbnailZoom.val = newZoom;
  };

  return div(
    // Preview Header
    div(
      { class: "flex justify-between items-center mb-md" },
      h3({ class: "text-lg font-semibold" }, `Preview (${format.label})`),
      div(
        { class: "flex items-center gap-sm" },
        button(
          {
            class: "btn btn-secondary",
            onclick: onPreview,
            disabled: previewGenerating.val,
          },
          previewGenerating.val ? "Generating..." : "Generate Preview",
        ),
        // Only show zoom controls for image content (thumbnails)
        previewData.val.pages && previewData.val.pages.length > 0
          ? div(
              { class: "zoom-control flex items-center gap-xs" },
              span(
                {
                  class: "zoom-label",
                  style:
                    "margin-right: 8px; font-size: 12px; color: var(--vscode-descriptionForeground); font-weight: 500;",
                },
                "Thumbnails:",
              ),
              button(
                {
                  class: "btn btn-secondary",
                  onclick: () => adjustThumbnailZoom(-25),
                  disabled: thumbnailZoom.val <= 50,
                  title: "Smaller thumbnails",
                },
                "−",
              ),
              span({ class: "text-xs font-medium" }, () => `${thumbnailZoom.val}%`),
              button(
                {
                  class: "btn btn-secondary",
                  onclick: () => adjustThumbnailZoom(25),
                  disabled: thumbnailZoom.val >= 300,
                  title: "Larger thumbnails",
                },
                "+",
              ),
              button(
                {
                  class: "btn btn-secondary",
                  onclick: () => {
                    thumbnailZoom.val = 100;
                  },
                  title: "Reset thumbnail size",
                },
                "100%",
              ),
            )
          : null,
      ),
    ),

    // Preview Content
    (() => {
      if (previewGenerating.val) {
        return PreviewLoading();
      }

      if (previewData.val.error) {
        return PreviewError(previewData.val.error, onPreview);
      }

      // Handle text content for text-based formats
      if (previewData.val.text) {
        return PreviewTextContent(previewData.val.text);
      }

      // Handle image pages for visual formats
      if (previewData.val.pages) {
        return PreviewPagesGrid(previewData.val.pages, thumbnailZoom.val, (page: PreviewPage) => {
          selectedImage.val = page;
        });
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

const PreviewError = (errorMessage: string, onRetry: () => void) => {
  return div(
    { class: "preview-error" },
    span("⚠️ Failed to generate preview"),
    span({ style: "font-size: 0.75rem;" }, errorMessage),
    button(
      {
        class: "btn btn-secondary",
        style: "margin-top: 0.5rem;",
        onclick: onRetry,
      },
      "Retry",
    ),
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
          onclick: onGenerate,
        },
        "Generate Preview",
      ),
    ),
  );
};

const PreviewPagesGrid = (
  pages: PreviewPage[],
  thumbnailZoom: number,
  onImageClick: (page: PreviewPage) => void,
) => {
  const baseSize = 200; // Base thumbnail size
  const scaledSize = Math.round(baseSize * (thumbnailZoom / 100));

  return div(
    {
      class: "preview-grid",
      style: `display: grid; grid-template-columns: repeat(auto-fill, minmax(${scaledSize}px, 1fr)); gap: 16px; padding: 16px;`,
    },
    ...pages.map((page) => PreviewPageCard(page, scaledSize, onImageClick)),
  );
};

const PreviewPageCard = (
  page: PreviewPage,
  thumbnailSize: number,
  onImageClick: (page: PreviewPage) => void,
) => {
  // Calculate responsive thumbnail dimensions based on zoom
  const maxThumbnailHeight = Math.round(thumbnailSize * 1.4); // Maintain aspect ratio expectation

  const thumbnailStyle = `
    width: 100%;
    height: auto;
    max-width: ${thumbnailSize}px;
    max-height: ${maxThumbnailHeight}px;
    object-fit: contain;
    border: 1px solid var(--vscode-widget-border);
    border-radius: 4px;
    cursor: pointer;
    display: block;
  `;

  const containerStyle = `
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px;
    border: 1px solid var(--vscode-widget-border);
    border-radius: 4px;
    background: var(--vscode-editor-background);
    cursor: pointer;
    transition: background-color 0.2s ease;
  `;

  const pageNumberStyle = `
    margin-top: 8px;
    font-size: 12px;
    color: var(--vscode-descriptionForeground);
    font-weight: 500;
    pointer-events: none;
  `;

  return div(
    {
      class: "preview-page",
      style: containerStyle,
      onclick: () => onImageClick(page),
      onmouseover: (e: Event) => {
        const target = e.currentTarget as HTMLElement;
        target.style.background = "var(--vscode-list-hoverBackground)";
      },
      onmouseout: (e: Event) => {
        const target = e.currentTarget as HTMLElement;
        target.style.background = "var(--vscode-editor-background)";
      },
    },
    img({
      class: "preview-page-image",
      src: page.imageData,
      alt: `Page ${page.pageNumber}`,
      loading: "lazy",
      style: thumbnailStyle,
    }),
    span(
      {
        class: "preview-page-number",
        style: pageNumberStyle,
      },
      `${page.pageNumber}`,
    ),
  );
};

const PreviewTextContent = (text: string) => {
  return div(
    { class: "preview-text-container" },
    div(
      {
        class: "preview-text-content",
        style: `
          max-height: 600px;
          overflow-y: auto;
          border: 1px solid var(--vscode-widget-border);
          border-radius: 4px;
          padding: 16px;
          background: var(--vscode-editor-background);
          color: var(--vscode-editor-foreground);
          font-family: var(--vscode-editor-font-family);
          font-size: 13px;
          line-height: 1.5;
          white-space: pre-wrap;
          word-wrap: break-word;
          margin: 8px 0;
        `,
      },
      text || "No text content available",
    ),
  );
};
