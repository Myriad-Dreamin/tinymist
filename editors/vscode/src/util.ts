import * as vscode from "vscode";
import * as path from "path";
import { ViewColumn } from "vscode";
import { readFile } from "fs/promises";

export const typstDocumentSelector = [
  { scheme: "file", language: "typst" },
  { scheme: "untitled", language: "typst" },
];

export function activeTypstEditor() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "typst") {
    return;
  }
  return editor;
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
