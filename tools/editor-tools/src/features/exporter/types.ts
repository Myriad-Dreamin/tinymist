export type Scalar = string | number | boolean;

export type ExportFormatId = "pdf" | "png" | "svg" | "html" | "markdown" | "tex" | "text" | "query";

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
  fileExtension: string;
  options: OptionSchema[];
}

export interface PreviewPage {
  pageNumber: number;
  imageData: string; // base64 encoded PNG
}

export interface PreviewData {
  // format: ExportFormatId;
  pages?: PreviewPage[];
  text?: string;
  error?: string;
}
