/**
 * Drag-and-drop or Copy-and-paste support.
 */

import * as vscode from "vscode";
import { dirname, extname, basename, relative } from "path";
import { typstDocumentSelector } from "../util";
import {
  Mime,
  typstSupportedMimes,
  PasteResourceKind,
  pasteResourceKinds as pasteResourceKinds,
  typstImageEditKind,
  typstPasteLinkEditKind,
  typstUriEditKind,
  Schemes,
} from "./drop-paste.def";
import { IContext } from "../context";

// Type guards for paste API availability
function hasDocumentPasteAPI(): boolean {
  return "registerDocumentPasteEditProvider" in vscode.languages;
}

// Conditional types for compatibility
type DocumentPasteEditProvider = any;
type DocumentPasteEditContext = any;
type DocumentPasteEdit = any;

type SubscriptionContext = Pick<IContext, "subscriptions">;

export function dragAndDropActivate(context: SubscriptionContext) {
  context.subscriptions.push(
    vscode.languages.registerDocumentDropEditProvider(typstDocumentSelector, new DropProvider()),
  );
}

export function copyAndPasteActivate(context: SubscriptionContext) {
  // Check if document paste API is available (VS Code 1.97+)
  if (!hasDocumentPasteAPI()) {
    console.warn(
      "Tinymist: Document paste API not available, copy/paste features will be disabled",
    );
    return;
  }

  const providedEditKinds = [typstPasteLinkEditKind, typstUriEditKind, typstImageEditKind];

  const sel = typstDocumentSelector;
  context.subscriptions.push(
    vscode.languages.registerDocumentPasteEditProvider(sel, new PasteUriProvider(), {
      providedPasteEditKinds: [typstPasteLinkEditKind],
      pasteMimeTypes: PasteUriProvider.mimeTypes,
    }),
    vscode.languages.registerDocumentPasteEditProvider(sel, new PasteResourceProvider(), {
      providedPasteEditKinds: providedEditKinds,
      pasteMimeTypes: PasteResourceProvider.mimeTypes,
    }),
  );
}

const enum DropPasteAction {
  Drop,
  Paste,
}

type EditClass<A extends DropPasteAction> = A extends DropPasteAction.Drop
  ? vscode.DocumentDropEdit
  : any; // Use any for DocumentPasteEdit to handle compatibility

interface ResolvedEdits {
  snippet: vscode.SnippetString;
  additionalEdits: vscode.WorkspaceEdit;
  yieldTo: any[]; // Use any[] for DocumentDropOrPasteEditKind compatibility
}

class DropOrPasteContext<A extends DropPasteAction> {
  title: string;
  editKind = typstUriEditKind;

  constructor(
    private kind: A,
    private context: DocumentPasteEditContext | undefined,
    private document: vscode.TextDocument,
    private token: vscode.CancellationToken,
  ) {
    if (this.kind === DropPasteAction.Drop) {
      this.title = "Drop (Typst)";
    } else {
      this.title = "Paste (Typst)";
    }
  }

  private readonly _yieldTo = (() => {
    // Check if DocumentDropOrPasteEditKind is available
    if (typeof (vscode as any).DocumentDropOrPasteEditKind !== "undefined") {
      return [
        (vscode as any).DocumentDropOrPasteEditKind.Text,
        (vscode as any).DocumentDropOrPasteEditKind.Empty.append(
          "typst",
          "link",
          "image",
          "attachment",
        ), // Prefer notebook attachments
      ];
    }
    return []; // Fallback for older VS Code versions
  })();

  resolved: ResolvedEdits[] = [];

  finalize(): EditClass<A>[] {
    return this.resolved.map((edit) => this.resolveOne(edit));
  }

  resolveOne(edit: ResolvedEdits): EditClass<A> {
    if (this.kind === DropPasteAction.Drop) {
      const dropEdit = new vscode.DocumentDropEdit(edit.snippet);
      dropEdit.additionalEdit = edit.additionalEdits;
      dropEdit.yieldTo = [...this._yieldTo, ...edit.yieldTo];
      return dropEdit as EditClass<A>;
    } else {
      // For paste, we need to handle the case where DocumentPasteEdit might not be available
      const DocumentPasteEdit = (vscode as any).DocumentPasteEdit;
      if (DocumentPasteEdit) {
        const pasteEdit = new DocumentPasteEdit(edit.snippet, this.title, this.editKind);
        pasteEdit.additionalEdit = edit.additionalEdits;
        pasteEdit.yieldTo = [...this._yieldTo, ...edit.yieldTo];
        return pasteEdit as EditClass<A>;
      } else {
        // Fallback - this should not happen if we check hasDocumentPasteAPI() first
        throw new Error("DocumentPasteEdit not available");
      }
    }
  }

  async transfer(
    ranges: readonly vscode.Range[],
    dataTransfer: vscode.DataTransfer,
  ): Promise<boolean> {
    {
      const mediaFiles = await this.takeMediaFiles(dataTransfer);
      if (mediaFiles) {
        const edit = await this.handleMediaFiles(ranges, mediaFiles);
        if (edit) {
          this.editKind = typstImageEditKind;
          this.resolved.push(edit);
          return this.wrapRangeAsLinkContent();
        }
      }

      const uriList = await this.takeUriList(dataTransfer);
      if (uriList) {
        const edit = await this.editByUriList(ranges, uriList, false);
        if (edit) {
          this.resolved.push(edit);
          return this.wrapRangeAsLinkContent();
        }
      }
    }

    return false;
  }

  async pasteUri(ranges: readonly vscode.Range[], dataTransfer: vscode.DataTransfer) {
    this.editKind = typstUriEditKind;
    this.title = "Paste Link (Typst)";
    const item = dataTransfer.get(Mime.textPlain);
    const text = await item?.asString();
    if (this.token.isCancellationRequested || !text) {
      return;
    }

    // TODO: If the user has explicitly requested to paste as a typst link,
    // try to paste even if we don't have a valid uri
    const uriText = findValidUriInText(text);
    if (!uriText) {
      return;
    }

    const uriList = UriList.from(uriText);
    const edit = await this.editByUriList(ranges, uriList, false);
    if (edit) {
      this.resolved.push(edit);
      return this.wrapRangeAsLinkContent();
    }

    return false;
  }

  wrapRangeAsLinkContent(): boolean {
    // todo: link content support
    // if (
    //   !(await shouldInsertMarkdownLinkByDefault(
    //     this._parser,
    //     document,
    //     settings.insert,
    //     ranges,
    //     token,
    //   ))
    // ) {
    //   edit.yieldTo.push(vscode.DocumentDropOrPasteEditKind.Empty.append("uri"));
    // }
    return true;
  }

  async takeMediaFiles(dataTransfer: vscode.DataTransfer): Promise<MediaFileEntry[] | undefined> {
    const pathGenerator = new NewFilePathGenerator();
    const fileEntries = coalesce(
      await Promise.all(
        Array.from(dataTransfer, async ([mime, item]): Promise<MediaFileEntry | undefined> => {
          if (!typstSupportedMimes.has(mime)) {
            return;
          }

          const file = item?.asFile();
          if (!file) {
            return;
          }

          if (file.uri) {
            // If the file is already in a workspace, we don't want to create a copy of it
            const workspaceFolder = vscode.workspace.getWorkspaceFolder(file.uri);
            if (workspaceFolder) {
              return { uri: file.uri };
            }
          }

          const newFile = await pathGenerator.getNewFilePath(this.document, file, this.token);
          if (!newFile) {
            return;
          }
          return { uri: newFile.uri, newFile: { contents: file, overwrite: newFile.overwrite } };
        }),
      ),
    );
    if (!fileEntries.length) {
      return;
    }

    return fileEntries;
  }

  async takeUriList(dataTransfer: vscode.DataTransfer): Promise<UriList | undefined> {
    const uriListData = await dataTransfer.get(Mime.textUriList)?.asString();
    if (!uriListData || this.token.isCancellationRequested) {
      return;
    }

    const uriList = UriList.from(uriListData);
    if (!uriList.entries.length) {
      return;
    }

    // In some browsers, copying from the address bar sets both text/uri-list and text/plain.
    // Disable ourselves if there's also a text entry with the same http(s) uri as our list,
    // unless we are explicitly requested.
    if (
      uriList.entries.length === 1 &&
      [Schemes.http, Schemes.https].includes(uriList.entries[0].uri.scheme as Schemes) &&
      !(this.context?.only as any)?.contains?.(typstUriEditKind)
    ) {
      const text = await dataTransfer.get(Mime.textPlain)?.asString();
      if (this.token.isCancellationRequested) {
        return;
      }

      if (text && textMatchesUriList(text, uriList)) {
        return;
      }
    }

    return uriList;
  }

  async handleMediaFiles(ranges: readonly vscode.Range[], mediaFiles: MediaFileEntry[]) {
    const mediaUriList = new UriList(
      mediaFiles.map((entry) => ({ uri: entry.uri, str: entry.uri.toString() })),
    );

    return this.editByUriList(ranges, mediaUriList, true, (additionalEdits) => {
      for (const entry of mediaFiles) {
        if (entry.newFile) {
          additionalEdits.createFile(entry.uri, {
            contents: entry.newFile.contents,
            overwrite: entry.newFile.overwrite,
          });
        }
      }
    });
  }

  async editByUriList(
    ranges: readonly vscode.Range[],
    uriList: UriList,
    isMedia: boolean,
    createAdditionalEdits?: (additionalEdits: vscode.WorkspaceEdit) => void,
  ): Promise<ResolvedEdits | undefined> {
    if (uriList.entries.length !== 1) {
      vscode.window.showErrorMessage("Only one URI can be pasted at a time.");
      return;
    }

    const additionalEdits = new vscode.WorkspaceEdit();
    if (createAdditionalEdits) {
      createAdditionalEdits(additionalEdits);
    }

    // Use 1 for all empty ranges but give non-empty range unique indices starting after 1
    let placeHolderStartIndex = 1 + uriList.entries.length;

    // Sort ranges by start position
    const orderedRanges = [...ranges].sort((a, b) => a.start.compareTo(b.start)).reverse();
    const allRangesAreEmpty = orderedRanges.every((range) => range.isEmpty);

    const additionalImports = new Set<string>();

    let resolved = true;
    for (const range of orderedRanges) {
      const snippetEdit = await this.createUriListSnippet(uriList, range, {
        isMedia,
        placeholderStartIndex: allRangesAreEmpty ? 1 : placeHolderStartIndex,
      });
      if (!snippetEdit) {
        resolved = false;
        continue;
      }

      const [snippet, imports] = snippetEdit;

      // insertedLinkCount += snippet.insertedLinkCount;
      // insertedImageCount += snippet.insertedImageCount;
      // insertedAudioCount += snippet.insertedAudioCount;
      // insertedVideoCount += snippet.insertedVideoCount;

      placeHolderStartIndex += uriList.entries.length;

      additionalEdits.replace(this.document.uri, range, snippet);
      for (const importLine of imports) {
        additionalImports.add(importLine);
      }
    }

    if (!resolved) {
      return;
    }

    const imports = Array.from(additionalImports).sort();
    if (imports.length > 0) {
      additionalEdits.insert(this.document.uri, new vscode.Position(0, 0), imports.join(""));
    }

    // label: edit.label,
    // kind: edit.kind,
    return {
      snippet: new vscode.SnippetString(""),
      additionalEdits,
      yieldTo: [],
    };
  }

  async createUriListSnippet(
    uriList: UriList,
    range: vscode.Range,
    exts: { isMedia: boolean; placeholderStartIndex: number },
  ) {
    if (uriList.entries.length !== 1) {
      vscode.window.showErrorMessage("Only one URI can be pasted at a time.");
      return;
    }

    const entry = uriList.entries[0];

    if (exts.isMedia || entry.uri.scheme === "file" || entry.uri.scheme === "untitled") {
      return this.createLocalUriListSnippet(entry.uri, range, exts);
    } else {
      return this.createRemoteUriListSnippet(entry.uri, range, exts);
    }
  }

  async createRemoteUriListSnippet(
    dragFileUri: vscode.Uri,
    range: vscode.Range,
    _exts: { placeholderStartIndex: number },
  ) {
    // todo: check valid plain url, because some url may contain invalid characters for http markup.
    if (range.isEmpty) {
      return;
    }

    const res = await vscode.commands.executeCommand<
      [{ mode: "math" | "markup" | "code" | "comment" | "string" | "raw" }]
    >("tinymist.interactCodeContext", {
      textDocument: {
        uri: this.document.uri.toString(),
      },
      query: [
        {
          kind: "modeAt",
          position: {
            line: range.start.line,
            character: range.start.character,
          },
        },
      ],
    });

    const linkText = dragFileUri.toString();
    const wrappedText = this.document.getText(range);
    let text = "";
    switch (res?.[0]?.mode || undefined) {
      case "markup":
        text = `#link("${escapeStr(linkText)}")[${wrappedText}]`;
        break;
      case "math":
        text = `#link("${escapeStr(linkText)}", $${wrappedText}$)`;
        break;
      case "code":
        text = `link("${escapeStr(linkText)}", {${wrappedText}})`;
        break;
      case "string":
      case "comment":
      case "raw":
        return undefined;
    }

    return [text, []] as const;
  }

  async createLocalUriListSnippet(
    dragFileUri: vscode.Uri,
    range: vscode.Range,
    _exts: { placeholderStartIndex: number },
  ) {
    const dropFileUri = this.document.uri;

    let dragFilePath = "";
    const workspaceFolder = vscode.workspace.getWorkspaceFolder(dragFileUri);
    if (dropFileUri.scheme === "untitled") {
      if (workspaceFolder) {
        dragFilePath = relative(workspaceFolder.uri.fsPath, dragFileUri.fsPath);
      }
    } else {
      dragFilePath = relative(dirname(dropFileUri.fsPath), dragFileUri.fsPath);
    }

    const barStrPath = escapeStr(dragFilePath.replace(/\\/g, "/"));
    const strPath = `"${barStrPath}"`;
    let codeSnippet = strPath;
    const resourceKind: PasteResourceKind | undefined =
      pasteResourceKinds[extname(dragFileUri.fsPath)];
    // todo: fetch latest version
    const additionalPkgs: [string, string, string | undefined][] = [];
    switch (resourceKind) {
      case PasteResourceKind.BuiltinImage:
        codeSnippet = `image(${strPath})`;
        break;
      case PasteResourceKind.Webp:
        additionalPkgs.push(["@preview/grayness", "0.1.0", "grayscale-image"]);
        codeSnippet = `grayscale-image(read(${strPath}))`;
        break;
      case PasteResourceKind.Xlsx:
        additionalPkgs.push(["@preview/rexllent", "0.3.0", "xlsx-parser"]);
        codeSnippet = `xlsx-parser(read(${strPath}, encoding: none))`;
        break;
      case PasteResourceKind.Ods:
        additionalPkgs.push(["@preview/spreet", "0.1.0", undefined]);
        additionalPkgs.push(["@preview/rexllent", "0.3.0", "spreet-parser"]);
        codeSnippet = `spreet-parser(spreet.decode(read(${strPath}, encoding: none)))`;
        break;
      case PasteResourceKind.Source:
        codeSnippet = `include ${strPath}`;
        break;
      case PasteResourceKind.Markdown:
        additionalPkgs.push(["@preview/cmarker", "0.1.1", undefined]);
        codeSnippet = `cmarker.render(read(${strPath}))`;
        break;
      case PasteResourceKind.TeX:
        additionalPkgs.push(["@preview/mitex", "0.2.4", "mitex"]);
        codeSnippet = `mitex(read(${strPath}))`;
        break;
      case PasteResourceKind.Json:
        codeSnippet = `json(${strPath})`;
        break;
      case PasteResourceKind.Toml:
        codeSnippet = `toml(${strPath})`;
        break;
      case PasteResourceKind.Csv:
        codeSnippet = `csv(${strPath})`;
        break;
      case PasteResourceKind.Yaml:
        codeSnippet = `yaml(${strPath})`;
        break;
      case PasteResourceKind.Bib:
        codeSnippet = `bibliography(${strPath})`;
        break;
      default:
        codeSnippet = `read(${strPath})`;
        break;
    }

    const res = await vscode.commands.executeCommand<
      [{ mode: "math" | "markup" | "code" | "comment" | "string" | "raw" }]
    >("tinymist.interactCodeContext", {
      textDocument: {
        uri: this.document.uri.toString(),
      },
      query: [
        {
          kind: "modeAt",
          position: {
            line: range.start.line,
            character: range.start.character,
          },
        },
      ],
    });

    let text = codeSnippet;
    switch (res?.[0]?.mode || undefined) {
      case "math":
      case "markup":
        text = `#${codeSnippet}`;
        break;
      case "code":
        text = codeSnippet;
        break;
      case "string":
        text = barStrPath;
        break;
      case "comment":
      case "raw":
      case undefined:
        return undefined;
    }

    const additionalImports = [];
    if (additionalPkgs.length > 0) {
      const t = this.document.getText();
      for (const [pkgName, version, importName] of additionalPkgs) {
        if (!t.includes(pkgName)) {
          if (importName) {
            additionalImports.push(`#import "${pkgName}:${version}": ${importName}\n`);
          } else {
            additionalImports.push(`#import "${pkgName}:${version}"\n`);
          }
        }
      }
    }

    // console.log(resourceKind, res?.[0]?.mode, codeSnippet, text);
    return [text, additionalImports] as const;
  }
}
const DropContext = DropOrPasteContext<DropPasteAction.Drop>;
const PasteContext = DropOrPasteContext<DropPasteAction.Paste>;

export class DropProvider implements vscode.DocumentDropEditProvider {
  async provideDocumentDropEdits(
    document: vscode.TextDocument,
    position: vscode.Position,
    dataTransfer: vscode.DataTransfer,
    token: vscode.CancellationToken,
  ): Promise<vscode.DocumentDropEdit[] | undefined> {
    const ctx = new DropContext(DropPasteAction.Drop, undefined, document, token);
    const ranges = [new vscode.Range(position, position)];

    const transferred = await ctx.transfer(ranges, dataTransfer);

    if (!transferred || token.isCancellationRequested) {
      return;
    }

    return ctx.finalize();
  }
}

export class PasteResourceProvider implements DocumentPasteEditProvider {
  public static readonly mimeTypes = [Mime.textUriList, "files", ...typstSupportedMimes];

  public async provideDocumentPasteEdits(
    document: vscode.TextDocument,
    ranges: readonly vscode.Range[],
    dataTransfer: vscode.DataTransfer,
    context: DocumentPasteEditContext,
    token: vscode.CancellationToken,
  ): Promise<DocumentPasteEdit[] | undefined> {
    const ctx = new PasteContext(DropPasteAction.Paste, context, document, token);

    const transferred = await ctx.transfer(ranges, dataTransfer);

    if (!transferred || token.isCancellationRequested) {
      return;
    }

    return ctx.finalize();
  }
}

export class PasteUriProvider implements DocumentPasteEditProvider {
  public static readonly mimeTypes = [Mime.textPlain];

  public async provideDocumentPasteEdits(
    document: vscode.TextDocument,
    ranges: readonly vscode.Range[],
    dataTransfer: vscode.DataTransfer,
    context: DocumentPasteEditContext,
    token: vscode.CancellationToken,
  ): Promise<DocumentPasteEdit[] | undefined> {
    const ctx = new PasteContext(DropPasteAction.Paste, context, document, token);

    const transferred = await ctx.pasteUri(ranges, dataTransfer);
    if (!transferred || token.isCancellationRequested) {
      return;
    }

    return ctx.finalize();
  }
}

type OverwriteBehavior = "overwrite" | "nameIncrementally";

export interface CopyFileConfiguration {
  readonly overwriteBehavior: OverwriteBehavior;
}

export function getCopyFileConfiguration(document: vscode.TextDocument): CopyFileConfiguration {
  const config = vscode.workspace.getConfiguration("tinymist", document);
  return {
    overwriteBehavior: readOverwriteBehavior(config),
  };
}

function readOverwriteBehavior(config: vscode.WorkspaceConfiguration): OverwriteBehavior {
  switch (config.get("copyFiles.overwriteBehavior")) {
    case "overwrite":
      return "overwrite";
    default:
      return "nameIncrementally";
  }
}

export class NewFilePathGenerator {
  private readonly _usedPaths = new Set<string>();

  async getNewFilePath(
    document: vscode.TextDocument,
    file: vscode.DataTransferFile,
    token: vscode.CancellationToken,
  ): Promise<{ readonly uri: vscode.Uri; readonly overwrite: boolean } | undefined> {
    const config = getCopyFileConfiguration(document);
    const desiredPath = getDesiredNewFilePath(document, file);

    const root = vscode.Uri.joinPath(desiredPath, "..");
    const ext = extname(desiredPath.fsPath);
    let baseName = basename(desiredPath.fsPath);
    baseName = baseName.slice(0, baseName.length - ext.length);
    for (let i = 0; ; ++i) {
      if (token.isCancellationRequested) {
        return undefined;
      }

      const name = i === 0 ? baseName : `${baseName}-${i}`;
      const uri = vscode.Uri.joinPath(root, name + ext);
      if (this._wasPathAlreadyUsed(uri)) {
        continue;
      }

      // Try overwriting if it already exists
      if (config.overwriteBehavior === "overwrite") {
        this._usedPaths.add(uri.toString());
        return { uri, overwrite: true };
      }

      // Otherwise we need to check the fs to see if it exists
      try {
        await vscode.workspace.fs.stat(uri);
      } catch {
        if (!this._wasPathAlreadyUsed(uri)) {
          // Does not exist
          this._usedPaths.add(uri.toString());
          return { uri, overwrite: false };
        }
      }
    }
  }

  private _wasPathAlreadyUsed(uri: vscode.Uri) {
    return this._usedPaths.has(uri.toString());
  }
}

export function getDesiredNewFilePath(
  document: vscode.TextDocument,
  file: vscode.DataTransferFile,
): vscode.Uri {
  const docUri = getParentDocumentUri(document.uri);

  // Default to next to current file
  return vscode.Uri.joinPath(docUri, "..", file.name);
}

function getParentDocumentUri(uri: vscode.Uri): vscode.Uri {
  if ((uri.scheme as Schemes) === Schemes.notebookCell) {
    // is notebook documents necessary?
    for (const notebook of vscode.workspace.notebookDocuments) {
      for (const cell of notebook.getCells()) {
        if (cell.document.uri.toString() === uri.toString()) {
          return notebook.uri;
        }
      }
    }
  }

  return uri;
}

interface MediaFileEntry {
  readonly uri: vscode.Uri;
  readonly newFile?: { readonly contents: vscode.DataTransferFile; readonly overwrite: boolean };
}

function textMatchesUriList(text: string, uriList: UriList): boolean {
  if (text === uriList.entries[0].str) {
    return true;
  }

  try {
    const uri = vscode.Uri.parse(text);
    return uriList.entries.some((entry) => entry.uri.toString() === uri.toString());
  } catch {
    return false;
  }
}

function splitUriList(str: string): string[] {
  return str.split("\r\n");
}

function parseUriList(str: string): string[] {
  return splitUriList(str)
    .filter((value) => !value.startsWith("#")) // Remove comments
    .map((value) => value.trim());
}

export class UriList {
  static from(str: string): UriList {
    return new UriList(
      coalesce(
        parseUriList(str).map((line) => {
          try {
            return { uri: vscode.Uri.parse(line), str: line };
          } catch {
            // Uri parse failure
            return undefined;
          }
        }),
      ),
    );
  }

  constructor(
    public readonly entries: ReadonlyArray<{ readonly uri: vscode.Uri; readonly str: string }>,
  ) {}
}

const externalUriSchemes: ReadonlySet<string> = new Set([
  Schemes.http,
  Schemes.https,
  Schemes.mailto,
  Schemes.file,
]);

export function findValidUriInText(text: string): string | undefined {
  const trimmedUrlList = text.trim();

  if (
    !/^\S+$/.test(trimmedUrlList) || // Uri must consist of a single sequence of characters without spaces
    !trimmedUrlList.includes(":") // And it must have colon somewhere for the scheme. We will verify the schema again later
  ) {
    return;
  }

  let uri: vscode.Uri;
  try {
    uri = vscode.Uri.parse(trimmedUrlList);
  } catch {
    // Could not parse
    return;
  }

  // `Uri.parse` is lenient and will return a `file:` uri even for non-uri text such as `abc`
  // Make sure that the resolved scheme starts the original text
  if (!trimmedUrlList.toLowerCase().startsWith(uri.scheme.toLowerCase() + ":")) {
    return;
  }

  // Only enable for an allow list of schemes. Otherwise this can be accidentally activated for non-uri text
  // such as `c:\abc` or `value:foo`
  if (!externalUriSchemes.has(uri.scheme.toLowerCase())) {
    return;
  }

  // Some part of the uri must not be empty
  // This disables the feature for text such as `http:`
  if (!uri.authority && uri.path.length < 2 && !uri.query && !uri.fragment) {
    return;
  }

  return trimmedUrlList;
}

function coalesce<T>(array: ReadonlyArray<T | undefined | null>): T[] {
  return <T[]>array.filter((e) => !!e);
}

function escapeStr(str: string): string {
  return str.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
