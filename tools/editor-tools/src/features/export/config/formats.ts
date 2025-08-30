import type { ExportFormat } from "../types";

export const EXPORT_FORMATS: ExportFormat[] = [
  {
    id: 'pdf',
    label: 'PDF',
    description: 'Portable Document Format - ideal for documents, print, and sharing',
    supportsPreview: true,
    fileExtension: 'pdf',
    options: [
      {
        key: 'pdf.creationTimestamp',
        type: 'string',
        label: 'Creation Timestamp',
        description: 'Set creation timestamp (leave empty for current time, "null" for no timestamp)',
      }
    ]
  },
  {
    id: 'png',
    label: 'PNG',
    description: 'Portable Network Graphics - high-quality images for web and presentations',
    supportsPreview: true,
    fileExtension: 'png',
    options: [
      {
        key: 'png.ppi',
        type: 'number',
        label: 'PPI (Pixels per inch)',
        description: 'Resolution for the exported image',
        default: 96,
        min: 72,
        max: 600
      },
      {
        key: 'png.fill',
        type: 'color',
        label: 'Background Fill',
        description: 'Background color for transparent areas',
        default: '#ffffff'
      },
      {
        key: 'png.merged',
        type: 'boolean',
        label: 'Merge Pages',
        description: 'Combine all pages into a single image',
        default: false
      },
      {
        key: 'png.merged.gap',
        type: 'string',
        label: 'Gap Between Pages',
        description: 'Space between pages when merged (e.g., "10pt", "5mm")',
        default: '0pt'
      }
    ]
  },
  {
    id: 'svg',
    label: 'SVG',
    description: 'Scalable Vector Graphics - perfect for web and scalable graphics',
    supportsPreview: true,
    fileExtension: 'svg',
    options: [
      {
        key: 'svg.merged',
        type: 'boolean',
        label: 'Merge Pages',
        description: 'Combine all pages into a single SVG',
        default: false
      },
      {
        key: 'svg.merged.gap',
        type: 'string',
        label: 'Gap Between Pages',
        description: 'Space between pages when merged (e.g., "10pt", "5mm")',
        default: '0pt'
      }
    ]
  },
  {
    id: 'html',
    label: 'HTML',
    description: 'HyperText Markup Language - for web publishing and online viewing',
    supportsPreview: false,
    fileExtension: 'html',
    options: []
  },
  {
    id: 'markdown',
    label: 'Markdown',
    description: 'Markdown format - for documentation and text processing',
    supportsPreview: false,
    fileExtension: 'md',
    options: [
      {
        key: 'markdown.processor',
        type: 'string',
        label: 'Processor',
        description: 'Typst file for custom processing (e.g., "/processor.typ")'
      },
      {
        key: 'markdown.assetsPath',
        type: 'string',
        label: 'Assets Path',
        description: 'Directory path for exported assets'
      }
    ]
  },
  {
    id: 'tex',
    label: 'TeX/LaTeX',
    description: 'TeX format - for LaTeX integration and academic publishing',
    supportsPreview: false,
    fileExtension: 'tex',
    options: [
      {
        key: 'tex.processor',
        type: 'string',
        label: 'Processor',
        description: 'Typst file for custom TeX processing (e.g., "/ieee-tex.typ")'
      },
      {
        key: 'tex.assetsPath',
        type: 'string',
        label: 'Assets Path',
        description: 'Directory path for exported assets'
      }
    ]
  },
  {
    id: 'text',
    label: 'Plain Text',
    description: 'Plain text format - extract text content only',
    supportsPreview: false,
    fileExtension: 'txt',
    options: []
  },
  {
    id: 'query',
    label: 'Query',
    description: 'Custom query export - extract specific data or metadata',
    supportsPreview: false,
    fileExtension: 'json',
    options: [
      {
        key: 'query.format',
        type: 'select',
        label: 'Output Format',
        description: 'Format for the query results',
        default: 'json',
        options: [
          { value: 'json', label: 'JSON' },
          { value: 'yaml', label: 'YAML' }
        ]
      },
      {
        key: 'query.outputExtension',
        type: 'string',
        label: 'File Extension',
        description: 'Custom file extension (without dot)',
        default: 'json'
      },
      {
        key: 'query.selector',
        type: 'string',
        label: 'Selector',
        description: 'Query selector (e.g., "heading", "figure.caption")',
        default: 'heading'
      },
      {
        key: 'query.field',
        type: 'string',
        label: 'Field',
        description: 'Field to extract from selected elements'
      },
      {
        key: 'query.strict',
        type: 'boolean',
        label: 'Strict Mode',
        description: 'Enable strict query parsing',
        default: false
      },
      {
        key: 'query.pretty',
        type: 'boolean',
        label: 'Pretty Print',
        description: 'Format output with indentation',
        default: true
      },
      {
        key: 'query.one',
        type: 'boolean',
        label: 'Single Result',
        description: 'Return only the first matching element',
        default: false
      }
    ]
  }
];

export function getFormatById(id: string): ExportFormat | undefined {
  return EXPORT_FORMATS.find(format => format.id === id);
}

export function getDefaultOptions(format: ExportFormat): Record<string, string | number | boolean | undefined> {
  const options: Record<string, string | number | boolean | undefined> = {};
  format.options.forEach(option => {
    if (option.default !== undefined) {
      options[option.key] = option.default;
    }
  });
  return options;
}
