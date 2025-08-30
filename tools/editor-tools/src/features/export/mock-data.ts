import type { ExportConfig, PreviewPage } from "./types";
import { EXPORT_FORMATS, getDefaultOptions } from "./config/formats";

// Mock export configuration for development
export const MOCK_EXPORT_CONFIG: ExportConfig = {
  format: EXPORT_FORMATS[0], // PDF format
  inputPath: "/workspace/document.typ",
  outputPath: "/workspace/document.pdf",
  options: getDefaultOptions(EXPORT_FORMATS[0])
};

// Mock preview pages for development
export const MOCK_PREVIEW_PAGES: PreviewPage[] = [
  {
    pageNumber: 1,
    imageData: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
    width: 612,
    height: 792
  },
  {
    pageNumber: 2,
    imageData: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
    width: 612,
    height: 792
  },
  {
    pageNumber: 3,
    imageData: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
    width: 612,
    height: 792
  }
];

// Mock current document info for development
export const MOCK_DOCUMENT_INFO = {
  path: "/workspace/document.typ",
  name: "document.typ",
  pageCount: 3,
  isTypstDocument: true
};
