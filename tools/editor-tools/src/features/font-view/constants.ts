/**
 * Simple constants for font view component
 */

export const FONT_DEFAULTS = {
  WEIGHT: 400,
  STRETCH: 1000,
  STYLE: "normal",
} as const;

export const FONT_WEIGHT_CATEGORIES = {
  thin: {
    label: "Thin",
    weight: 100,
  },
  extralight: {
    label: "Extra Light",
    weight: 200,
  },
  light: {
    label: "Light",
    weight: 300,
  },
  normal: {
    label: "Normal",
    weight: 400,
  },
  medium: {
    label: "Medium",
    weight: 500,
  },
  semibold: {
    label: "Semi Bold",
    weight: 600,
  },
  bold: {
    label: "Bold",
    weight: 700,
  },
  extrabold: {
    label: "Extra Bold",
    weight: 800,
  },
  black: {
    label: "Black",
    weight: 900,
  },
} as const;

export const FONT_STRETCH_CATEGORIES = {
  condensed: {
    label: "Condensed",
    test: (stretch: number) => stretch < 1000,
  },
  normal: {
    label: "Normal Width",
    test: (stretch: number) => stretch === 1000,
  },
  expanded: {
    label: "Expanded",
    test: (stretch: number) => stretch > 1000,
  },
} as const;

export const FONT_STYLE_OPTIONS = {
  normal: { label: "Normal" },
  italic: { label: "Italic" },
  oblique: { label: "Oblique" },
} as const;
