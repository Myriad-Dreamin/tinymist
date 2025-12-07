import type { ExportFormat, OptionSchema } from "./types";

const PAGES_OPT: OptionSchema = {
  key: "pages",
  type: "string",
  label: "Page Range",
  description: 'Page range to export (e.g., "1-3,5,7-9", leave empty for all pages)',
  default: "",
  validate: validatePageRanges,
};

const IMAGE_PAGES_OPTS: OptionSchema[] = [
  PAGES_OPT,
  {
    key: "pageNumberTemplate",
    type: "string",
    label: "Page Number Template",
    description:
      'Template used to render page numbers when exporting multiple pages (e.g., "Page {n}")',
    default: "",
  },
];

const MERGE_OPTS: OptionSchema[] = [
  {
    key: "merged",
    type: "boolean",
    label: "Merge Pages",
    description: "Combine selected pages into a single image",
    default: false,
  },
  {
    key: "merged.gap",
    type: "string",
    label: "Gap Between Pages",
    description: 'Space between pages when merged (e.g., "10pt", "5mm")',
    default: "0pt",
    dependsOn: "merged", // Only show when merged is true
  },
];

export const EXPORT_FORMATS: ExportFormat[] = [
  {
    id: "pdf",
    label: "PDF",
    fileExtension: "pdf",
    options: [
      PAGES_OPT,
      {
        key: "pdf.creationTimestamp",
        type: "datetime",
        label: "Creation Timestamp",
        description: "The document's creation date (leave empty for current time)",
        default: "",
        validate: (value: string) => {
          if (value.trim() === "") {
            return; // Allow empty input
          }
          if (!/^\d+$/.test(value)) {
            return "Creation timestamp must be a valid non-negative integer UNIX timestamp";
          }
          const num = Number(value);
          if (Number.isNaN(num) || !Number.isInteger(num) || num < 0) {
            return "Creation timestamp must be a valid non-negative integer UNIX timestamp";
          }
        },
      },
      {
        key: "pdf.pdfStandard",
        type: "select",
        label: "PDF Standards",
        description: "Optional multiple PDF standards to enforce (e.g. PDF/A, PDF/UA)",
        default: [],
        multiple: true,
        options: [
          { value: "1.4", label: "PDF 1.4" },
          { value: "1.5", label: "PDF 1.5" },
          { value: "1.6", label: "PDF 1.6" },
          { value: "1.7", label: "PDF 1.7" },
          { value: "2.0", label: "PDF 2.0" },
          { value: "a-1b", label: "PDF/A-1b" },
          { value: "a-1a", label: "PDF/A-1a" },
          { value: "a-2b", label: "PDF/A-2b" },
          { value: "a-2u", label: "PDF/A-2u" },
          { value: "a-2a", label: "PDF/A-2a" },
          { value: "a-3b", label: "PDF/A-3b" },
          { value: "a-3u", label: "PDF/A-3u" },
          { value: "a-3a", label: "PDF/A-3a" },
          { value: "a-4", label: "PDF/A-4" },
          { value: "a-4f", label: "PDF/A-4f" },
          { value: "a-4e", label: "PDF/A-4e" },
          { value: "ua-1", label: "PDF/UA-1" },
        ],
      },
      {
        key: "pdf.pdfTags",
        type: "boolean",
        label: "PDF Tags",
        description: "Include tagged structure in the PDF for better accessibility.",
        default: true,
      },
    ],
  },
  {
    id: "png",
    label: "PNG",
    fileExtension: "png",
    options: [
      ...IMAGE_PAGES_OPTS,
      ...MERGE_OPTS,
      {
        key: "png.ppi",
        type: "number",
        label: "PPI (Pixels per inch)",
        description: "Resolution for the exported image",
        default: 144,
        min: 0,
        validate: (value: string) => {
          const num = Number(value);
          if (Number.isNaN(num) || !Number.isFinite(num) || num <= 0) {
            return "PPI must be a valid positive number";
          }
        },
      },
      {
        key: "png.fill",
        type: "color",
        label: "Background Fill",
        description: "Background color for transparent areas (use CLI instead if alpha is needed)",
        default: "#ffffff",
      },
    ],
  },
  {
    id: "svg",
    label: "SVG",
    fileExtension: "svg",
    options: [...IMAGE_PAGES_OPTS, ...MERGE_OPTS],
  },
  {
    id: "html",
    label: "HTML",
    fileExtension: "html",
    options: [],
  },
  {
    id: "markdown",
    label: "Markdown",
    fileExtension: "md",
    options: [
      {
        key: "markdown.processor",
        type: "string",
        label: "Processor",
        description: 'Typst file for custom processing (e.g., "/processor.typ")',
        default: "",
      },
      {
        key: "markdown.assetsPath",
        type: "string",
        label: "Assets Path",
        description: "Directory path for exported assets",
        default: "",
      },
    ],
  },
  {
    id: "tex",
    label: "TeX/LaTeX",
    fileExtension: "tex",
    options: [
      {
        key: "tex.processor",
        type: "string",
        label: "Processor",
        description: 'Typst file for custom TeX processing (e.g., "/ieee-tex.typ")',
        default: "",
      },
      {
        key: "tex.assetsPath",
        type: "string",
        label: "Assets Path",
        description: "Directory path for exported assets",
        default: "",
      },
    ],
  },
  {
    id: "text",
    label: "Plain Text",
    fileExtension: "txt",
    options: [],
  },
  {
    id: "query",
    label: "Query",
    fileExtension: "json",
    options: [
      {
        key: "query.format",
        type: "select",
        label: "Output Format",
        description: "Format for the query results",
        default: "json",
        options: [
          { value: "json", label: "JSON" },
          { value: "yaml", label: "YAML" },
        ],
      },
      {
        key: "query.outputExtension",
        type: "string",
        label: "File Extension",
        description: "Custom file extension (without dot)",
        default: "json",
      },
      {
        key: "query.selector",
        type: "string",
        label: "Selector",
        description: 'Query selector (e.g., "heading", "figure.caption")',
        default: "heading",
      },
      {
        key: "query.field",
        type: "string",
        label: "Field",
        description: "Field to extract from selected elements",
        default: "",
      },
      {
        key: "query.strict",
        type: "boolean",
        label: "Strict Mode",
        description: "Enable strict query parsing",
        default: false,
      },
      {
        key: "query.pretty",
        type: "boolean",
        label: "Pretty Print",
        description: "Format output with indentation",
        default: true,
      },
      {
        key: "query.one",
        type: "boolean",
        label: "Single Result",
        description: "Return only the first matching element",
        default: false,
      },
    ],
  },
];

function validatePageRanges(value: string): string | undefined {
  if (!value.trim()) {
    return; // Allow empty input
  }
  const parts = value
    .split(",")
    .map((p) => p.trim())
    .filter((p) => p);
  for (const part of parts) {
    const rangeParts = part.split("-").map((s) => s.trim());
    if (rangeParts.length > 2) {
      return `Invalid page range format: ${part}`;
    }
    if (rangeParts.length === 1) {
      // Single page
      const num = parseInt(rangeParts[0], 10);
      if (Number.isNaN(num) || num <= 0) {
        return `Invalid page number: ${part}`;
      }
    } else {
      // Range
      const [startStr, endStr] = rangeParts;
      let startNum: number | undefined;
      let endNum: number | undefined;
      if (startStr) {
        startNum = parseInt(startStr, 10);
        if (Number.isNaN(startNum) || startNum <= 0) {
          return `Invalid page range: ${part}`;
        }
      }
      if (endStr) {
        endNum = parseInt(endStr, 10);
        if (Number.isNaN(endNum) || endNum <= 0) {
          return `Invalid page range: ${part}`;
        }
      }
      if (startNum !== undefined && endNum !== undefined && startNum > endNum) {
        return `Invalid page range: ${part}`;
      }
      // If both start and end are empty, invalid
      if (!startStr && !endStr) {
        return `Invalid page range: ${part}`;
      }
    }
  }
}
