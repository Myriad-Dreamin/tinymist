export interface FsFontSource {
  kind: "fs";
  path: string;
}

export interface MemoryFontSource {
  kind: "memory";
  name: string;
}

export type FontSource = FsFontSource | MemoryFontSource;
