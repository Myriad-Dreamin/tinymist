import * as vscode from "vscode";

export const enum Schemes {
  http = "http",
  https = "https",
  file = "file",
  untitled = "untitled",
  mailto = "mailto",
  vscode = "vscode",
  "vscode-insiders" = "vscode-insiders",
  notebookCell = "vscode-notebook-cell",
}

export enum PasteResourceKind {
  BuiltinImage,
  Webp,
  Source,
  Markdown,
  TeX,
  Json,
  Toml,
  Csv,
  Yaml,
  Bib,
  Xlsx,
  Ods,
}

export const pasteResourceKinds: Record<string, PasteResourceKind> = {
  ".avif": PasteResourceKind.BuiltinImage,
  ".bmp": PasteResourceKind.BuiltinImage,
  ".gif": PasteResourceKind.BuiltinImage,
  ".ico": PasteResourceKind.BuiltinImage,
  ".jpe": PasteResourceKind.BuiltinImage,
  ".jpg": PasteResourceKind.BuiltinImage,
  ".jpeg": PasteResourceKind.BuiltinImage,
  ".png": PasteResourceKind.BuiltinImage,
  ".psd": PasteResourceKind.BuiltinImage,
  ".svg": PasteResourceKind.BuiltinImage,
  ".tga": PasteResourceKind.BuiltinImage,
  ".tif": PasteResourceKind.BuiltinImage,
  ".tiff": PasteResourceKind.BuiltinImage,
  ".webp": PasteResourceKind.Webp,
  ".typst": PasteResourceKind.Source,
  ".typ": PasteResourceKind.Source,
  ".md": PasteResourceKind.Markdown,
  ".tex": PasteResourceKind.TeX,
  ".json": PasteResourceKind.Json,
  ".jsonc": PasteResourceKind.Json,
  ".json5": PasteResourceKind.Json,
  ".toml": PasteResourceKind.Toml,
  ".csv": PasteResourceKind.Csv,
  ".yaml": PasteResourceKind.Yaml,
  ".yml": PasteResourceKind.Yaml,
  ".bib": PasteResourceKind.Bib,
  ".xlsx": PasteResourceKind.Xlsx,
  ".ods": PasteResourceKind.Ods,
};

// Helper function to check if DocumentDropOrPasteEditKind is available
function hasDocumentDropOrPasteEditKind(): boolean {
  return typeof (vscode as any).DocumentDropOrPasteEditKind !== "undefined";
}

// Lazy evaluation of edit kinds to handle backward compatibility
function createEditKinds() {
  if (!hasDocumentDropOrPasteEditKind()) {
    // Return null values for older VS Code versions
    return {
      typstPasteLinkEditKind: null,
      typstUriEditKind: null,
      typstImageEditKind: null,
    };
  }

  const DocumentDropOrPasteEditKind = (vscode as any).DocumentDropOrPasteEditKind;
  const pasteLinkKind = DocumentDropOrPasteEditKind.Empty.append("typst", "link");
  
  return {
    typstPasteLinkEditKind: pasteLinkKind,
    typstUriEditKind: pasteLinkKind.append("uri"),
    typstImageEditKind: pasteLinkKind.append("image"),
  };
}

// Cache the edit kinds
let _editKinds: ReturnType<typeof createEditKinds> | null = null;

function getEditKinds() {
  if (_editKinds === null) {
    _editKinds = createEditKinds();
  }
  return _editKinds;
}

// Export getter functions instead of constants to avoid immediate evaluation
export function getTypstPasteLinkEditKind() {
  return getEditKinds().typstPasteLinkEditKind;
}

export function getTypstUriEditKind() {
  return getEditKinds().typstUriEditKind;
}

export function getTypstImageEditKind() {
  return getEditKinds().typstImageEditKind;
}

export const Mime = {
  textUriList: "text/uri-list",
  textPlain: "text/plain",
} as const;

export const typstSupportedMimes = new Set([
  "image/avif",
  "image/bmp",
  "image/gif",
  "image/jpeg",
  "image/png",
  "image/webp",
  // "video/mp4",
  // "video/ogg",
  // "audio/mpeg",
  // "audio/aac",
  // "audio/x-wav",
]);
