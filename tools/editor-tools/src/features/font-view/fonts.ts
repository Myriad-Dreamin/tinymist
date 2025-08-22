import type { FontSource } from "@/types";

export interface FontInfo {
  index?: number;
  name: string;
  fixedFamily?: string;
  source?: number;
  style?: string;
  stretch?: number;
  weight?: number;
}

export interface FontFamily {
  name: string;
  infos: FontInfo[];
}

export interface FontResources {
  sources: FontSource[];
  families: FontFamily[];
}

export interface FontFilters {
  searchQuery: string;
  weightFilter: string;
  styleFilter: string;
  stretchFilter: string;
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
