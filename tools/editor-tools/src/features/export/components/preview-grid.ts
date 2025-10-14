import van, { type State } from "vanjs-core";
import { requestGeneratePreview } from "../../../vscode";
import type { ExportConfig, PreviewPage } from "../types";

const { div, h3, img, span, button } = van.tags;

interface PreviewGridProps {
  exportConfig: State<ExportConfig>;
  previewPages: State<PreviewPage[]>;
}

export const PreviewGrid = (props: PreviewGridProps) => {
  const { exportConfig, previewPages } = props;

  const { format } = exportConfig.val;
  const isLoading = van.state<boolean>(false);
  const error = van.state<string | null>(null);
  const thumbnailZoom = van.state<number>(100); // Percentage for thumbnail sizing
  const textContent = van.state<string | null>(null); // For text-based formats
  const selectedImage = van.state<PreviewPage | null>(null); // For image modal

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
    if (typeof window !== "undefined") {
      window.addEventListener("message", handleMessage);

      // Cleanup function would be called when component is destroyed
      // In VanJS, we can use the reactive pattern for cleanup
    }
  };

  // Set up listeners when component is first created
  van.derive(() => {
    setupPreviewListeners();
  });

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
            onclick: generatePreview,
            disabled: isLoading.val,
          },
          isLoading.val ? "Generating..." : "Generate Preview",
        ),
        // Only show zoom controls for image content (thumbnails)
        previewPages.val.length > 0
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
      if (isLoading.val) {
        return PreviewLoading();
      }

      if (error.val) {
        return PreviewError(error.val, generatePreview);
      }

      // Handle text content for text-based formats
      if (textContent.val) {
        return PreviewTextContent(textContent.val);
      }

      // Handle image pages for visual formats
      if (previewPages.val.length === 0) {
        return PreviewEmpty(generatePreview);
      }

      return PreviewPagesGrid(previewPages.val, thumbnailZoom.val, (page: PreviewPage) => {
        selectedImage.val = page;
      });
    })(),

    // Image Modal
    selectedImage.val
      ? ImageModal(selectedImage.val, () => {
          selectedImage.val = null;
        })
      : null,
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

const ImageModal = (page: PreviewPage, onClose: () => void) => {
  const modalStyle = `
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    cursor: pointer;
  `;

  const imageStyle = `
    max-width: min(100%, 80vw);
    max-height: min(100%, 80vh);
    object-fit: contain;
    border-radius: 4px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
    cursor: default;
  `;

  const closeButtonStyle = `
    position: absolute;
    top: 20px;
    right: 20px;
    background: rgba(255, 255, 255, 0.9);
    border: none;
    border-radius: 50%;
    width: 40px;
    height: 40px;
    font-size: 20px;
    font-weight: bold;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.2s ease;
  `;

  const infoStyle = `
    position: absolute;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(0, 0, 0, 0.7);
    color: white;
    padding: 8px 16px;
    border-radius: 4px;
    font-size: 14px;
  `;

  // Add keyboard event listener for ESC key
  const handleKeydown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    }
  };

  // Add event listener when modal is created
  if (typeof window !== "undefined") {
    window.addEventListener("keydown", handleKeydown);
    // Note: In a real app, you'd want to clean this up when the modal is destroyed
  }

  return div(
    {
      class: "image-modal",
      style: modalStyle,
      onclick: onClose,
    },
    button(
      {
        style: closeButtonStyle,
        onclick: (e: Event) => {
          e.stopPropagation();
          onClose();
        },
        onmouseover: (e: Event) => {
          const target = e.currentTarget as HTMLElement;
          target.style.background = "rgba(255, 255, 255, 1)";
        },
        onmouseout: (e: Event) => {
          const target = e.currentTarget as HTMLElement;
          target.style.background = "rgba(255, 255, 255, 0.9)";
        },
      },
      "×",
    ),
    img({
      src: page.imageData,
      alt: `Page ${page.pageNumber} - Large View`,
      style: imageStyle,
      onclick: (e: Event) => e.stopPropagation(),
    }),
    div(
      {
        style: infoStyle,
      },
      `Page ${page.pageNumber} (${page.width}×${page.height})`,
    ),
  );
};
