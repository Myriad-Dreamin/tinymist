export interface ExportPdfOpts {
  pages?: string[];
  creationTimestamp?: string | null;
  pdfStandard?: string[];
  noPdfTags?: boolean;
}

export interface PageMergeOpts {
  gap?: string | null;
}

export interface ExportPngOpts {
  pages?: string[];
  pageNumberTemplate?: string;
  merge?: PageMergeOpts;
  fill?: string;
  ppi?: number;
}

export interface ExportSvgOpts {
  pages?: string[];
  pageNumberTemplate?: string;
  merge?: PageMergeOpts;
}

export interface ExportTypliteOpts {
  processor?: string;
  assetsPath?: string;
}

export interface ExportQueryOpts {
  format: string;
  outputExtension?: string;
  strict?: boolean;
  pretty?: boolean;
  selector: string;
  field?: string;
  one?: boolean;
}

// biome-ignore lint/suspicious/noEmptyInterface: no fields yet
export interface ExportHtmlOpts { }

// biome-ignore lint/suspicious/noEmptyInterface: no fields yet
export interface ExportTextOpts { }

export type ExportOpts =
  | ExportPdfOpts
  | ExportPngOpts
  | ExportSvgOpts
  | ExportTypliteOpts
  | ExportQueryOpts
  | ExportHtmlOpts
  | ExportTextOpts;

export interface ExportActionOpts {
  write?: boolean;
  open?: boolean;
  template?: string;
}
