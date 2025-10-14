import type { ExportFormat, OptionSchema, Scalar } from "../types";

const PAGES_OPT: OptionSchema = {
  key: "pages",
  type: "string",
  label: "Page Range",
  description: 'Page range to export (e.g., "1-3,5,7-9", leave empty for all pages)',
  default: "",
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
    description: "Portable Document Format - ideal for documents, print, and sharing",
    supportsPreview: true,
    fileExtension: "pdf",
    options: [
      PAGES_OPT,
      {
        key: "pdf.creationTimestamp",
        type: "string",
        label: "Creation Timestamp",
        description:
          'Set creation timestamp (leave empty for current time, "null" for no timestamp)',
        default: "",
      },
    ],
  },
  {
    id: "png",
    label: "PNG",
    description: "Portable Network Graphics - high-quality images for web and presentations",
    supportsPreview: true,
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
      },
      {
        key: "png.fill",
        type: "color",
        label: "Background Fill",
        description: "Background color for transparent areas",
        default: "#ffffff",
      },
    ],
  },
  {
    id: "svg",
    label: "SVG",
    description: "Scalable Vector Graphics - perfect for web and scalable graphics",
    supportsPreview: true,
    fileExtension: "svg",
    options: [PAGES_OPT, ...MERGE_OPTS],
  },
  {
    id: "html",
    label: "HTML",
    description: "HyperText Markup Language - for web publishing and online viewing",
    supportsPreview: false,
    fileExtension: "html",
    options: [],
  },
  {
    id: "markdown",
    label: "Markdown",
    description: "Markdown format - for documentation and text processing",
    supportsPreview: true,
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
    description: "TeX format - for LaTeX integration and academic publishing",
    supportsPreview: true,
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
    description: "Plain text format - extract text content only",
    supportsPreview: true,
    fileExtension: "txt",
    options: [],
  },
  {
    id: "query",
    label: "Query",
    description: "Custom query export - extract specific data or metadata",
    supportsPreview: true,
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

export function getFormatById(id: string): ExportFormat | undefined {
  return EXPORT_FORMATS.find((format) => format.id === id);
}

export function getDefaultOptions(format: ExportFormat): Record<string, Scalar | undefined> {
  const options: Record<string, Scalar | undefined> = {};
  format.options.forEach((option) => {
    if (option.default !== undefined) {
      options[option.key] = option.default;
    }
  });
  return options;
}
