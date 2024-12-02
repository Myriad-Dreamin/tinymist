export interface FsFontSource {
  kind: "fs";
  path: string;
}

export interface MemoryFontSource {
  kind: "memory";
  name: string;
}

export type FontSource = FsFontSource | MemoryFontSource;

function almost(value: number, target: number, threshold = 0.01) {
  return Math.abs(value - target) < threshold;
}

export function humanStyle(style?: string) {
  if (!style) {
    return "Regular";
  }

  if (style === "italic") {
    return "Italic";
  }

  if (style === "oblique") {
    return "Oblique";
  }

  return `Style ${style}`;
}

export function humanWeight(weight?: number) {
  if (!weight) {
    return "Regular";
  }

  if (almost(weight, 100)) {
    return "Thin";
  }

  if (almost(weight, 200)) {
    return "Extra Light";
  }

  if (almost(weight, 300)) {
    return "Light";
  }

  if (almost(weight, 400)) {
    return "Regular";
  }

  if (almost(weight, 500)) {
    return "Medium";
  }

  if (almost(weight, 600)) {
    return "Semibold";
  }

  if (almost(weight, 700)) {
    return "Bold";
  }

  if (almost(weight, 800)) {
    return "Extra Bold";
  }

  if (almost(weight, 900)) {
    return "Black";
  }

  return `Weight ${weight}`;
}

export function humanStretch(stretch?: number) {
  if (!stretch) {
    return "Normal";
  }

  if (almost(stretch, 500)) {
    return "Ultra-condensed";
  }

  if (almost(stretch, 625)) {
    return "Extra-condensed";
  }

  if (almost(stretch, 750)) {
    return "Condensed";
  }

  if (almost(stretch, 875)) {
    return "Semi-condensed";
  }

  if (almost(stretch, 1000)) {
    return "Normal";
  }

  if (almost(stretch, 1125)) {
    return "Semi-expanded";
  }

  if (almost(stretch, 1250)) {
    return "Expanded";
  }

  if (almost(stretch, 1500)) {
    return "Extra-expanded";
  }

  if (almost(stretch, 2000)) {
    return "Ultra-expanded";
  }

  return `${stretch}`;
}
