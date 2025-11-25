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
  // Custom validation function for string fields
  validate?: (value: string) => string | undefined; // Returns error message or undefined if valid
}

export interface ExportFormat {
  id: ExportFormatId;
  label: string;
  fileExtension: string;
  options: OptionSchema[];
}

export interface PreviewPage {
  pageNumber: number; // zero-based
  imageData: string; // base64 encoded PNG
}

export interface PreviewData {
  // format: ExportFormatId;
  pages?: PreviewPage[];
  text?: string;
  error?: string;
}
