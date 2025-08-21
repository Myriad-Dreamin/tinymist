import type { FontSource } from "../../types";

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
