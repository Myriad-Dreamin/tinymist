export type Scalar = string | number | boolean;

export type ExportFormatId = "pdf" | "png" | "svg" | "html" | "markdown" | "tex" | "text" | "query";

export interface OptionSchema {
  key: string;
  type: "string" | "number" | "boolean" | "color" | "datetime" | "select";
  label: string;
  description?: string;
  // default can be a single scalar or an array of scalars for multi-select
  default: Scalar | Scalar[];
  options?: Array<{ value: Scalar; label: string }>;
  // when true and type === 'select', UI should render a multi-select
  multiple?: boolean;
  min?: number;
  max?: number;
  dependsOn?: string; // Key of another option that this option depends on
  // Custom validation function for string fields.
  // Returns a string error message when invalid, or undefined when valid.
  validate?: (value: string) => string | undefined;
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
