// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import { previewActivate, previewDeactivate } from "./preview";

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
  return previewActivate(context, true);
}

// This method is called when your extension is deactivated
export function deactivate() {
  return previewDeactivate();
}
