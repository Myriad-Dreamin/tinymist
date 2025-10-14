export type ExportFormatId = "pdf" | "png" | "svg" | "html" | "markdown" | "tex" | "text" | "query";

export interface OptionSchema {
  key: string;
  type: "string" | "number" | "boolean" | "color" | "select";
  label: string;
  description?: string;
  default?: string | number | boolean;
  options?: Array<{ value: string | number | boolean; label: string }>;
  min?: number;
  max?: number;
  dependsOn?: string; // Key of another option that this option depends on
}

export interface ExportFormat {
  id: ExportFormatId;
  label: string;
  description: string;
  supportsPreview: boolean;
  fileExtension: string;
  options: OptionSchema[];
}

export interface FormatOptions {
  [key: string]: string | number | boolean | undefined;
}

export interface ExportConfig {
  format: ExportFormat;
  inputPath: string;
  outputPath: string;
  options: FormatOptions;
}

export interface PreviewPage {
  pageNumber: number;
  imageData: string; // base64 encoded PNG
  width: number;
  height: number;
}

export interface TaskDefinition {
  type: "typst";
  command: "export";
  label: string;
  group: "build";
  export: Record<string, string | number | boolean | undefined>;
}

export interface ExportResult {
  success: boolean;
  outputPath?: string;
  error?: string;
}
