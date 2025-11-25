import type { FontSource } from "@/types";

export const FONT_WEIGHT_CATEGORIES = {
  thin: {
    label: "Thin",
    value: 100,
  },
  extralight: {
    label: "Extra Light",
    value: 200,
  },
  light: {
    label: "Light",
    value: 300,
  },
  normal: {
    label: "Normal",
    value: 400,
  },
  medium: {
    label: "Medium",
    value: 500,
  },
  semibold: {
    label: "Semi Bold",
    value: 600,
  },
  bold: {
    label: "Bold",
    value: 700,
  },
  extrabold: {
    label: "Extra Bold",
    value: 800,
  },
  black: {
    label: "Black",
    value: 900,
  },
} as const;

export type FontWeight = keyof typeof FONT_WEIGHT_CATEGORIES;

export const FONT_STRETCH_CATEGORIES = {
  "ultra-condensed": {
    label: "Ultra Condensed",
    value: 500,
  },
  "extra-condensed": {
    label: "Extra Condensed",
    value: 625,
  },
  condensed: {
    label: "Condensed",
    value: 750,
  },
  "semi-condensed": {
    label: "Semi Condensed",
    value: 875,
  },
  normal: {
    label: "Normal",
    value: 1000,
  },
  "semi-expanded": {
    label: "Semi Expanded",
    value: 1125,
  },
  expanded: {
    label: "Expanded",
    value: 1250,
  },
  "extra-expanded": {
    label: "Extra Expanded",
    value: 1500,
  },
  "ultra-expanded": {
    label: "Ultra Expanded",
    value: 2000,
  },
} as const;

export type FontStretch = keyof typeof FONT_STRETCH_CATEGORIES;

export const FONT_STYLE_CATEGORIES = {
  normal: { label: "Normal", value: 0 },
  italic: { label: "Italic", value: 1 },
  oblique: { label: "Oblique", value: 2 },
} as const;

export type FontStyle = keyof typeof FONT_STYLE_CATEGORIES;

export const FONT_DEFAULTS = {
  WEIGHT: 400,
  STRETCH: 1000,
  STYLE: "normal",
} as const;

export interface FontInfo {
  index?: number;
  fixedFamily?: string;
  name: string;
  source: number;
  style: string;
  stretch: number;
  weight: number;
}

export interface FontFamily {
  name: string;
  infos: FontInfo[];
}

export interface FontResources {
  sources: FontSource[];
  families: FontFamily[];
}

/**
 * Extracts file information from font source
 */
export function getFontFileInfo(
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
 * Rounds the given stretch value to the nearest pre-defined variant.
 */
export function roundStretch(stretch: number): FontStretch {
  if (stretch <= 562) return "ultra-condensed";
  if (stretch <= 687) return "extra-condensed";
  if (stretch <= 812) return "condensed";
  if (stretch <= 937) return "semi-condensed";
  if (stretch <= 1062) return "normal";
  if (stretch <= 1187) return "semi-expanded";
  if (stretch <= 1374) return "expanded";
  if (stretch <= 1749) return "extra-expanded";
  return "ultra-expanded";
}
