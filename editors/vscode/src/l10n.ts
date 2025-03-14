import * as vscode from "vscode";

export function l10nStr(message: string, args?: Record<string, string | number | boolean>): string {
  return vscode.l10n.t(message, args || {});
}
