import * as vscode from "vscode";
import { ExtensionContext } from "vscode";

export async function activate(context: ExtensionContext): Promise<void> {
  vscode.window.showInformationMessage("Hello World from HTML in typst!");
}

export async function deactivate(): Promise<void> {
  vscode.window.showInformationMessage("Bye, HTML in typst!");
}
