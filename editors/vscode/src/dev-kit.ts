import * as vscode from "vscode";

export function devKitActivate(context: vscode.ExtensionContext) {
  vscode.commands.executeCommand("setContext", "ext.tinymistDevKit", true);

  const devKitProvider = new DevKitProvider();
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.dev-kit", devKitProvider),
  );
}

class DevKitProvider implements vscode.TreeDataProvider<DevKitItem> {
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
        title: "Run Preview Dev",
        command: "tinymist.previewDev",
        tooltip: `Run Preview in Developing Mode. It sets data plane port to the fix default value.`,
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
