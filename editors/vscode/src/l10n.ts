import * as vscode from "vscode";

export function l10nStr(message: string, args?: Record<string, string | number | boolean>): string {
  console.log("vscode.l10n.uri", vscode.l10n.uri);
  return vscode.l10n.t(message, args || {});
}
