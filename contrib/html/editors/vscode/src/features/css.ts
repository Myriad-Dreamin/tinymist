import * as vscode from "vscode";
import { CssCompletionItemProvider } from "../css/cssCompletionItemProvider";

export function cssActivate(context: vscode.ExtensionContext) {
  let provider = new CssCompletionItemProvider();

  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((e) => {
      if (e.languageId === "css") {
        provider.refreshCompletionItems();
      }
    }),
  );

  return provider;
}
