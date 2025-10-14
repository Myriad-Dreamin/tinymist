import { State } from "vanjs-core";

export type ExportFormatId = "pdf" | "png" | "svg" | "html" | "markdown" | "tex" | "text" | "query";

export type Scalar = string | number | boolean;

export interface OptionSchema {
  key: string;
  type: "string" | "number" | "boolean" | "color" | "select";
  label: string;
  description?: string;
  default: Scalar;
  options?: Array<{ value: Scalar; label: string }>;
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
  [key: string]: Scalar | undefined;
}

export interface ExportConfig {
  format: ExportFormat;
  inputPath: string;
  outputPath: string;
  options: FormatOptions;
}

export interface ExportConfigState {
  inputPath: State<string>;
  outputPath: State<string>;
  format: State<ExportFormat>;
  options: Record<string, State<Scalar | undefined>>;
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
  export: Record<string, Scalar | undefined>;
}

export interface ExportResult {
  success: boolean;
  outputPath?: string;
  error?: string;
}
