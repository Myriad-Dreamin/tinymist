import van, { type ChildDom, type State } from "vanjs-core";
import { humanStretch, humanStyle, humanWeight } from "@/utils/font-format";
import { copyToClipboard, requestRevealPath, requestTextEdit } from "@/vscode";
import { FONT_DEFAULTS } from "../constants";
import type { FontFamily, FontInfo, FontResources } from "../fonts";
import { getFontFileInfo } from "../fonts";

const { div, a, span, button } = van.tags;

/**
 * Font variant component displaying individual font information
 */
const FontSlot = (
  font: FontInfo,
  fontResources: FontResources,
  showNumberOpt: { showNumber: boolean },
) => {
  const { fileName, filePath, canReveal } = getFontFileInfo(font, fontResources);

  return div(
    { class: "font-variant-item" },
    div(
      { class: "flex flex-1 justify-between items-center gap-md" },
      div(
        { class: "flex flex-wrap items-center gap-sm" },
        span(
          { class: "badge", style: `font-weight: ${font.weight}` },
          humanWeight(font.weight, showNumberOpt),
        ),
        font.stretch && font.stretch !== FONT_DEFAULTS.STRETCH
          ? span(
              { class: "badge", style: `font-stretch: ${font.stretch}` },
              humanStretch(font.stretch, showNumberOpt),
            )
          : null,
        font.style && font.style !== FONT_DEFAULTS.STYLE
          ? span({ class: "badge", style: `font-style: ${font.style}` }, humanStyle(font.style))
          : null,
      ),
      canReveal
        ? a(
            {
              class: "font-file-info font-mono clickable",
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
        : span({ class: "font-file-info font-mono" }, fileName),
    ),
  );
};

/**
 * Font action button component with activation animation
 */
const FontAction = (body: ChildDom, title: string, onclick: (this: HTMLButtonElement) => void) => {
  return button(
    {
      class: "btn",
      title,
      onclick,
    },
    body,
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
    FontAction("Insert", "Insert font name string at cursor", () => {
      requestTextEdit({
        newText: {
          kind: "by-mode",
          markup: `#${quotedName}`,
          rest: quotedName,
        },
      });
    }),
    FontAction("#set", "Insert as font set-rule", () => {
      const setRule = `set text(font: ${quotedName})`;
      requestTextEdit({
        newText: {
          kind: "by-mode",
          markup: `#${setRule}`,
          rest: setRule,
        },
      });
    }),
  ];
}

/**
 * Font family card component displaying family and its variants
 */
const FontFamilySlot = (
  family: FontFamily,
  fontResources: FontResources,
  showNumberOpt: { showNumber: boolean },
) => {
  if (!family?.name) {
    console.warn("FontFamilySlot: Invalid family data", family);
    return div({ class: "font-family-card card error" }, "Invalid font family data");
  }

  return div(
    { class: "font-family-card card" },
    div(
      { class: "flex justify-between items-start gap-md" },
      div({ class: "text-lg font-bold" }, family.name),
      div({ class: "flex flex-wrap gap-sm" }, ...createFontActions(family)),
    ),
    div(
      { style: "margin-top: 0.5rem" },
      div(
        { class: "text-desc text-sm" },
        `${family.infos.length} variant${family.infos.length !== 1 ? "s" : ""}:`,
      ),
      ...family.infos.map((font) => FontSlot(font, fontResources, showNumberOpt)),
    ),
  );
};

export const FontList =
  (
    filteredFamilies: State<FontFamily[]>,
    fontResources: State<FontResources>,
    showNumber: State<boolean>,
  ) =>
  () => {
    const showNumberOpt = { showNumber: showNumber.val };
    return div(
      { class: "list-highlight flex flex-col gap-sm" },
      filteredFamilies.val.length === 0
        ? div(
            { class: "text-center text-desc italic", style: "padding: 40px 20px" },
            "No fonts match the current filters",
          )
        : filteredFamilies.val.map((family: FontFamily) =>
            FontFamilySlot(family, fontResources.val, showNumberOpt),
          ),
    );
  };
