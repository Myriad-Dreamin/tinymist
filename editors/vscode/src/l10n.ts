import * as vscode from "vscode";

export function l10nMsg(message: string, args?: Record<string, string | number | boolean>): string {
  return vscode.l10n.t(message, args || {});
}
