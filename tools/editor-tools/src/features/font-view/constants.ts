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
    label: "Thin & Light",
    test: (weight: number) => weight <= 300,
  },
  normal: {
    label: "Normal",
    test: (weight: number) => weight >= 350 && weight <= 450,
  },
  medium: {
    label: "Medium",
    test: (weight: number) => weight >= 500 && weight <= 550,
  },
  semibold: {
    label: "Semi Bold",
    test: (weight: number) => weight >= 600 && weight <= 650,
  },
  bold: {
    label: "Bold & Heavy",
    test: (weight: number) => weight >= 700,
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

export const FONT_STYLE_OPTIONS = [
  { value: "", label: "All Styles" },
  { value: "normal", label: "Normal" },
  { value: "italic", label: "Italic" },
  { value: "oblique", label: "Oblique" },
] as const;
