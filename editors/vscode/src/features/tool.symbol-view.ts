import * as vscode from "vscode";
import { ExtensionContext } from "../state";
import { editorToolAt } from "./tool";

export class SymbolViewProvider implements vscode.WebviewViewProvider {
  static readonly Name = "tinymist.side-symbol-view";

  constructor(private context: ExtensionContext) {}

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken,
  ) {
    webviewView.webview.options = {
      // Allow scripts in the webview
      enableScripts: true,
    };

    editorToolAt(this.context, "symbol-view", webviewView);
  }
}
