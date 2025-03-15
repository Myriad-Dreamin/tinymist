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

/** Base kind for any sort of markdown link, including both path and media links */
export const typstPasteLinkEditKind = vscode.DocumentDropOrPasteEditKind.Empty.append(
  "typst",
  "link",
);

/** Kind for normal markdown links, i.e. include "path/to/file.typ" */
export const typstUriEditKind = typstPasteLinkEditKind.append("uri");

export const typstImageEditKind = typstPasteLinkEditKind.append("image");

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
