import type { ExportFormat, OptionSchema } from "./types";

const PAGES_OPT: OptionSchema = {
  key: "pages",
  type: "string",
  label: "Page Range",
  description: 'Page range to export (e.g., "1-3,5,7-9", leave empty for all pages)',
  default: "",
  validate: validatePageRanges,
};

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
        type: "string",
        label: "Creation Timestamp",
        description:
          "The document's creation date formatted as a UNIX timestamp. (leave empty for current time)",
        default: "",
        validate: (value: string) => {
          if (value.trim() === "") {
            return; // Allow empty input
          }
          const num = Number(value);
          if (Number.isNaN(num) || !Number.isInteger(num) || num < 0) {
            // fixme: it still accepts floating point numbers like "1e5"
            return "Creation timestamp must be a valid non-negative integer UNIX timestamp";
          }
        },
      },
    ],
  },
  {
    id: "png",
    label: "PNG",
    fileExtension: "png",
    options: [
      PAGES_OPT,
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
    options: [PAGES_OPT, ...MERGE_OPTS],
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
