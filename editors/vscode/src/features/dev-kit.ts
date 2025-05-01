import * as vscode from "vscode";
import { IContext } from "../context";

export function devKitActivate(context: IContext) {
  vscode.commands.executeCommand("setContext", "ext.tinymistDevKit", true);
  context.subscriptions.push({
    dispose: () => {
      vscode.commands.executeCommand("setContext", "ext.tinymistDevKit", false);
    },
  });

  const devKitProvider = new DevKitViewProvider();
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.dev-kit", devKitProvider),
  );
}

class DevKitViewProvider implements vscode.TreeDataProvider<DevKitItem> {
  constructor() {}

  refresh(): void {}

  getTreeItem(element: DevKitItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: DevKitItem): Thenable<DevKitItem[]> {
    if (element) {
      return Promise.resolve([]);
    }

    return Promise.resolve([
      new DevKitItem({
        title: "Runs Preview Dev",
        command: "tinymist.previewDev",
        tooltip: `Runs Preview in Developing Mode. It sets data plane port to the fix default value.`,
      }),
      new DevKitItem({
        title: "Runs Default Preview",
        command: "tinymist.startDefaultPreview",
        tooltip: `Runs Default Preview.`,
      }),
    ]);
  }
}

export class DevKitItem extends vscode.TreeItem {
  constructor(
    public readonly command: vscode.Command,
    public description = "",
  ) {
    super(command.title, vscode.TreeItemCollapsibleState.None);
    this.tooltip = this.command.tooltip || ``;
  }

  contextValue = "devkit-item";
}
