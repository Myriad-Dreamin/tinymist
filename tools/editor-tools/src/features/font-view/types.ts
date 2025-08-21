import type { FontSource } from "../../types";

export interface FontInfo {
  name: string;
  style?: string;
  weight?: number;
  stretch?: number;
  source?: number;
  index?: number;
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
