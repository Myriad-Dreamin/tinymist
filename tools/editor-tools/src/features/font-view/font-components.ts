import van, { type ChildDom, type PropsWithKnownKeys, type State } from "vanjs-core";
import { humanStretch, humanStyle, humanWeight } from "../../utils/font-format";
import { copyToClipboard, requestRevealPath, requestTextEdit } from "../../vscode";
import { FONT_DEFAULTS } from "./constants";
import type { FontFamily, FontInfo, FontResources } from "./types";

const { div, a, span, button, h3, p } = van.tags;

/**
 * Font action button component with activation animation
 */
export const FontAction = (
  icon: ChildDom,
  title: string,
  onclick: (this: HTMLButtonElement) => void,
  opts?: PropsWithKnownKeys<HTMLButtonElement> & {
    active?: State<boolean>;
    variant?: string;
  },
) => {
  const classProp = opts?.active
    ? van.derive(() => {
        let classes = "font-action-button";
        if (opts.variant === "toggle") classes += " toggle-button";
        if (opts.active?.val) classes += " activated";
        return classes;
      })
    : opts?.variant === "toggle"
      ? "font-action-button toggle-button"
      : "font-action-button";

  return button(
    {
      ...opts,
      class: classProp,
      title,
      onclick,
    },
    icon,
  );
};

/**
 * Extracts file information from font source
 */
function getFileInfo(
  font: FontInfo,
  fontResources: FontResources,
): {
  fileName: string;
  filePath: string;
  canReveal: boolean;
} {
  if (typeof font.source !== "number") {
    return {
      fileName: "Unknown source",
      filePath: "",
      canReveal: false,
    };
  }

  const source = fontResources.sources[font.source];
  if (!source) {
    return {
      fileName: "Invalid source",
      filePath: "",
      canReveal: false,
    };
  }

  if (source.kind === "fs") {
    return {
      fileName: source.path.split(/[\\/]/g).pop() || "Unknown file",
      filePath: source.path,
      canReveal: true,
    };
  } else {
    return {
      fileName: `Embedded: ${source.name}`,
      filePath: source.name,
      canReveal: false,
    };
  }
}

/**
 * Font variant component displaying individual font information
 */
export const FontSlot = (
  font: FontInfo,
  fontResources: FontResources,
  showNumberOpt: { showNumber: boolean },
) => {
  const { fileName, filePath, canReveal } = getFileInfo(font, fontResources);

  const fileInfoElement = canReveal
    ? a(
        {
          class: "font-file-info clickable",
          title: `Click to reveal in file explorer:\n${filePath}`,
          onclick() {
            const source = fontResources.sources[font.source as number];
            if (source?.kind === "fs") {
              requestRevealPath(source.path);
            }
          },
        },
        fileName,
      )
    : span({ class: "font-file-info" }, fileName);

  return div(
    { class: "font-variant-item" },
    div(
      { class: "font-variant-info" },
      div(
        { class: "font-variant-details" },
        span({ class: "font-detail-tag weight-tag" }, humanWeight(font.weight, showNumberOpt)),
        font.stretch && font.stretch !== FONT_DEFAULTS.STRETCH
          ? span(
              { class: "font-detail-tag stretch-tag" },
              humanStretch(font.stretch, showNumberOpt),
            )
          : null,
        font.style && font.style !== FONT_DEFAULTS.STYLE
          ? span({ class: "font-detail-tag style-tag" }, humanStyle(font.style))
          : null,
      ),
      fileInfoElement,
    ),
  );
};

/**
 * Creates font action buttons for font family
 */
function createFontActions(family: FontFamily) {
  const quotedName = `"${family.name.replace(/"/g, '\\"')}"`;
  return [
    FontAction("Copy", "Copy font family name", () => {
      copyToClipboard(family.name);
    }),
    FontAction("Insert", "Insert font name at cursor", () => {
      requestTextEdit({
        newText: {
          kind: "by-mode",
          markup: quotedName,
          code: quotedName,
        },
      });
    }),
    FontAction("#set", "Insert as font setting", () => {
      const markup = `#set text(font: ${quotedName})`;
      requestTextEdit({
        newText: {
          kind: "by-mode",
          markup,
          code: markup,
        },
      });
    }),
  ];
}

/**
 * Font family card component displaying family and its variants
 */
export const FontFamilySlot = (
  family: FontFamily,
  fontResources: FontResources,
  showNumberOpt: { showNumber: boolean },
) => {
  if (!family?.name) {
    console.warn("FontFamilySlot: Invalid family data", family);
    return div({ class: "font-family-card error" }, "Invalid font family data");
  }

  const variantCount = family.infos.length;
  const variantText = `${variantCount} variant${variantCount !== 1 ? "s" : ""}:`;

  return div(
    { class: "font-family-card" },
    div(
      { class: "font-family-header" },
      h3({ class: "font-family-name" }, family.name),
      div({ class: "font-family-actions" }, ...createFontActions(family)),
    ),
    div(
      { class: "font-variants-container" },
      p({ class: "font-variant-summary" }, variantText),
      ...family.infos.map((font) => FontSlot(font, fontResources, showNumberOpt)),
    ),
  );
};
