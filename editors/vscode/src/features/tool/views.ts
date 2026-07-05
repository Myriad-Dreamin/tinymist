import * as vscode from "vscode";
import type { ExtensionContext } from "../../state";
import { updateEditorToolView } from "../tool";
import { tools } from "./registry";

export class ToolViewProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  refresh(): void {}

  getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(): Thenable<vscode.TreeItem[]> {
    return Promise.resolve(
      tools
        .filter((tool) => tool.command)
        .map((tool) => {
          if (tool.command) {
            return new CommandItem(tool.command);
          }
          return undefined;
        })
        .filter((item): item is CommandItem => item !== undefined),
    );
  }
}

class CommandItem extends vscode.TreeItem {
  constructor(
    public readonly command: vscode.Command,
    public description = "",
  ) {
    super(command.title, vscode.TreeItemCollapsibleState.None);
    this.tooltip = this.command.tooltip || ``;
  }

  iconPath = new vscode.ThemeIcon("tools");

  contextValue = "tool-command";
}

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

    updateEditorToolView(this.context, "symbol-view", webviewView);
  }
}
