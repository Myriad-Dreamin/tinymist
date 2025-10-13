import * as vscode from "vscode";
import * as path from "path";
import { ViewColumn } from "vscode";
import { readFile } from "fs/promises";
import { isGitpod, translateGitpodURL } from "./gitpod";
import { strict as nativeAssert } from "assert";

export const typstDocumentSelector = [
  { scheme: "file", language: "typst" },
  { scheme: "untitled", language: "typst" },
];

export function assert(condition: boolean, explanation: string): asserts condition {
  try {
    nativeAssert(condition, explanation);
  } catch (err) {
    console.error(`Assertion failed:`, explanation);
    throw err;
  }
}

const bytes2utf8 = new TextDecoder("utf-8");
const utf82bytes = new TextEncoder();

/** Base64 to bytes
 * @param encoded Base64 encoded string
 * @returns bytes
 */
export const base64DecodeToBytes = (encoded: string) =>
  Uint8Array.from(atob(encoded), (m) => m.charCodeAt(0));

/**
 * Base64 to UTF-8
 * @param encoded Base64 encoded string
 * @returns UTF-8 string
 */
export const base64Decode = (encoded: string) => bytes2utf8.decode(base64DecodeToBytes(encoded));

/**
 * bytes to Base64
 * @param utf8Str bytes
 * @returns Base64 encoded string
 */
export const bytesBase64Encode = (bytes: Uint8Array) =>
  btoa(Array.from(bytes, (c) => String.fromCharCode(c)).join(""));

/**
 * UTF-8 to Base64
 * @param utf8Str UTF-8 string
 * @returns Base64 encoded string
 */
export const base64Encode = (utf8Str: string) => bytesBase64Encode(utf82bytes.encode(utf8Str));

export function substituteTemplateString(html: string, entries: { [key: string]: string }) {
  for (const [key, value] of Object.entries(entries)) {
    html = html.replace(key, base64Encode(value));
  }
  return html;
}

export function translateExternalURL(urlStr: string): string {
  if (isGitpod()) {
    return translateGitpodURL(urlStr);
  } else {
    return urlStr;
  }
}

export function activeTypstEditor() {
  const editor = vscode.window.activeTextEditor;
  if (!isTypstDocument(editor?.document)) {
    return;
  }
  return editor;
}

export function isTypstDocument(
  document: vscode.TextDocument | undefined,
): document is vscode.TextDocument & { languageId: "typst" } {
  return document?.languageId === "typst" && !document.uri.scheme.startsWith("tinymist");
}

export function getTargetViewColumn(viewColumn: ViewColumn | undefined): ViewColumn {
  if (viewColumn === ViewColumn.One) {
    return ViewColumn.Two;
  }
  if (viewColumn === ViewColumn.Two) {
    return ViewColumn.One;
  }
  return ViewColumn.Beside;
}

export function getSensibleTextEditorColumn(): ViewColumn {
  let editor = vscode.window.activeTextEditor;
  if (!editor) {
    // first visible editor
    if (vscode.window.visibleTextEditors.length > 0) {
      editor = vscode.window.visibleTextEditors[0];
    }
  }
  return editor?.viewColumn !== undefined ? editor.viewColumn : ViewColumn.Beside;
}

export async function loadHTMLFile(context: vscode.ExtensionContext, relativePath: string) {
  const filePath = path.resolve(context.extensionPath, relativePath);
  const fileContents = await readFile(filePath, "utf8");
  return fileContents;
}

export class DisposeList {
  disposes: (() => void)[] = [];
  disposed = false;
  constructor() {}
  add(d: (() => void) | vscode.Disposable) {
    if (this.disposed) {
      // console.error("disposed", this.taskId, "for", this.filePath);
      return;
    }

    if (typeof d === "function") {
      this.disposes.push(d);
    } else {
      this.disposes.push(() => d.dispose());
    }
  }
  dispose() {
    if (this.disposed) {
      return;
    }
    this.disposed = true;

    for (const d of this.disposes) {
      d();
    }
  }
}

export class VirtualConsole {
  writeEmitter = new vscode.EventEmitter<string>();
  writeRaw(str: string) {
    this.writeEmitter.fire(str);
  }
  write(str: string) {
    this.writeEmitter.fire(str.replace(/\n/g, "\r\n"));
  }
  writeln(str: string) {
    this.write(str);
    this.writeRaw("\r\n");
  }
}
