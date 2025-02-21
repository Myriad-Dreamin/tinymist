"use strict";
import * as vscode from "vscode";
import aggregator from "./cssAggregator";

export class CssCompletionItemProvider {
  public completionItems?: PromiseLike<vscode.CompletionItem[]>;

  constructor() {
    this.refreshCompletionItems();
  }

  //   public provideCompletionItems(
  //     document: vscode.TextDocument,
  //     position: vscode.Position,
  //     token: vscode.CancellationToken,
  //   ): Thenable<vscode.CompletionItem[]> {
  //     if (canTriggerCompletion(document, position)) {
  //       return this.completionItems as PromiseLike<vscode.CompletionItem[]>;
  //     } else {
  //       return Promise.reject<vscode.CompletionItem[]>("Not inside html class attribute.");
  //     }
  //   }

  public refreshCompletionItems() {
    this.completionItems = aggregator().then((cssClasses) => {
      const completionItems = cssClasses.map((cssClass) => {
        const completionItem = new vscode.CompletionItem(cssClass);
        completionItem.detail = `Insert ${cssClass}`;
        completionItem.insertText = cssClass;
        completionItem.kind = vscode.CompletionItemKind.Value;

        // make sure our completion item group are first
        completionItem.preselect = true;
        return completionItem;
      });
      return completionItems;
    });
  }
}
