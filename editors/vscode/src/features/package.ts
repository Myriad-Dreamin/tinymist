import * as vscode from "vscode";
import { PackageInfo, SymbolInfo, tinymist } from "../lsp";
import { getTargetViewColumn } from "../util";

export function packageFeatureActivate(context: vscode.ExtensionContext) {
  const packageView = new PackageViewProvider();
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.package-view", packageView),
    vscode.commands.registerCommand(
      "tinymist.showPackageDocsInternal",
      async (pkg: PackageInfo) => {
        console.log("show package docs", pkg);
        //
        try {
          const docs = await tinymist.getResource("/package/docs", pkg);
          console.log("docs", docs);

          const content = (await vscode.commands.executeCommand(
            "markdown.api.render",
            docs,
          )) as string;

          const activeEditor = vscode.window.activeTextEditor;

          // Create and show a new WebView
          const panel = vscode.window.createWebviewPanel(
            "typst-docs", // 标识符
            `@${pkg.namespace}/${pkg.name}:${pkg.version} (Documentation)`, // 面板标题
            getTargetViewColumn(activeEditor?.viewColumn),
            {
              enableScripts: false, // 启用 JS
              retainContextWhenHidden: true,
              enableFindWidget: true,
            },
          );

          panel.webview.html = `<html>
  <head>
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src https:; script-src 'nonce-${panel.webview.cspSource}';">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta charset="UTF-8">
    <title>${pkg.namespace}/${pkg.name}:${pkg.version}</title>
  </head>
  <body>
  ${content}
  </body>
  </html>
  `;
        } catch (e) {
          console.error("show package docs error", e);
          vscode.window.showErrorMessage(`Failed to show package documentation: ${e}`);
        }
      },
    ),
  );
}

class PackageViewProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  constructor() {}

  refresh(): void {}

  getTreeItem(element: NamespaceItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: any): Thenable<vscode.TreeItem[]> {
    if (element && CommandsItem.is(element)) {
      return this.getCommands();
    } else if (element && NamespaceItem.is(element)) {
      return this.getNsPackages(element.namespace);
    } else if (element && PackageGroupItem.is(element)) {
      return Promise.resolve(element.packages);
    } else if (element && PackageItem.is(element)) {
      return this.getPackageActions(element);
    } else if (element && SymbolsItem.is(element)) {
      return this.getPackageSymbols(element);
    } else if (element && SymbolItem.is(element)) {
      console.log("symbol item children", element);
      if (!element.info.children) {
        return Promise.resolve([]);
      }
      return Promise.resolve(createPackageSymbols(element.pkg, element.info.children));
    } else if (element) {
      return Promise.resolve([]);
    }

    return Promise.resolve([
      new CommandsItem(),
      ...["preview", "local"].map((ns) => new NamespaceItem(ns)),
    ]);
  }

  private async getCommands(): Promise<CommandsItem[]> {
    return [
      new CommandItem({
        title: "Create Local Package",
        command: "tinymist.createLocalPackage",
        tooltip: `Create a Typst local package.`,
      }),
      new CommandItem({
        title: "Open Local Package",
        command: "tinymist.openLocalPackage",
        tooltip: `Open a Typst local package.`,
      }),
    ];
  }

  private async getNsPackages(ns: string): Promise<NamespaceItem[]> {
    const packages = await tinymist.getResource("/package/by-namespace", ns);

    // group by name
    const groups = new Map<string, PackageItem[]>();
    for (const pkg of packages) {
      const group = groups.get(pkg.name) || [];
      group.push(new PackageItem(pkg));
      groups.set(pkg.name, group);
    }

    return Array.from(groups.entries()).map(([name, packages]) => {
      return new PackageGroupItem(ns, name, packages);
    });
  }

  async getPackageSymbols(element: SymbolsItem): Promise<vscode.TreeItem[]> {
    return createPackageSymbols(
      element.pkg,
      await tinymist.getResource("/package/symbol", element.pkg.pkg),
    );
  }

  private async getPackageActions(pkg: PackageItem): Promise<vscode.TreeItem[]> {
    return [
      new CommandItem({
        title: "Documentation",
        command: "tinymist.showPackageDocsInternal",
        arguments: [pkg.pkg],
        tooltip: `Open package documentation to side.`,
      }),
      new CommandItem({
        title: "Open",
        command: "vscode.openFolder",
        arguments: [vscode.Uri.file(pkg.pkg.path), { forceNewWindow: true }],
        tooltip: `Open the package directory in editor.`,
      }),
      new CommandItem({
        title: "Reveal in File Explorer",
        command: "revealFileInOS",
        arguments: [vscode.Uri.file(pkg.pkg.path)],
        tooltip: `Reveal the directory of the package in File Explorer.`,
      }),
      new SymbolsItem(pkg),
    ];
  }
}

export class CommandsItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is CommandsItem {
    return element.contextValue === "package-commands";
  }

  constructor(public description = "") {
    super(`commands`, vscode.TreeItemCollapsibleState.Collapsed);
    this.tooltip = `package commands`;
  }

  contextValue = "package-commands";
}

export class CommandItem extends vscode.TreeItem {
  constructor(
    public readonly command: vscode.Command,
    public description = "",
  ) {
    super(command.title, vscode.TreeItemCollapsibleState.None);
    this.tooltip = this.command.tooltip || ``;
  }

  iconPath = new vscode.ThemeIcon("tools");

  contextValue = "package-command";
}

export class NamespaceItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is NamespaceItem {
    return element.contextValue === "package-namespace-item";
  }

  constructor(
    public readonly namespace: string,
    public description = "",
  ) {
    super(`@${namespace}`, vscode.TreeItemCollapsibleState.Collapsed);
    this.tooltip = `namespace: ${namespace}`;
  }

  contextValue = "package-namespace-item";
}

export class PackageGroupItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is PackageGroupItem {
    return element.contextValue === "package-group-item";
  }

  constructor(
    public readonly namespace: string,
    public readonly name: string,
    public readonly packages: PackageItem[],
    public description = `@${namespace}/${name}`,
  ) {
    super(`${name}`, vscode.TreeItemCollapsibleState.Collapsed);
    this.tooltip = `package: @${namespace}/${name}`;
  }

  contextValue = "package-group-item";
}

export class PackageItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is PackageItem {
    return element.contextValue === "package-item";
  }

  constructor(
    public readonly pkg: PackageInfo,
    public description = "",
  ) {
    super(`${pkg.version}`, vscode.TreeItemCollapsibleState.Collapsed);
    this.tooltip = `package: @${pkg.namespace}/${pkg.name}:${pkg.version}`;
  }

  pkgId() {
    return `@${this.pkg.namespace}/${this.pkg.name}:${this.pkg.version}`;
  }

  contextValue = "package-item";
}

export class SymbolsItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is SymbolsItem {
    return element.contextValue === "package-symbols-item";
  }

  constructor(
    public readonly pkg: PackageItem,
    public description = "",
  ) {
    super(`symbols`, vscode.TreeItemCollapsibleState.Collapsed);
    this.tooltip = `symbols in package: ${pkg.pkgId()}`;
  }

  contextValue = "package-symbols-item";
}

export class SymbolItem extends vscode.TreeItem {
  static is(element: vscode.TreeItem): element is SymbolItem {
    return element.contextValue === "package-symbol-item";
  }

  constructor(
    public readonly pkg: PackageItem,
    public readonly info: SymbolInfo,
    public description = "",
  ) {
    const state =
      info.children.length > 0
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None;
    super(`${info.name}`, state);
    this.tooltip = `a symbol \`${info.name}\` in package: ${pkg.pkgId()}`;
    this.iconPath = new vscode.ThemeIcon("symbol-" + info.kind);
  }

  contextValue = "package-symbol-item";
}

function createPackageSymbols(pkgItem: PackageItem, bases: SymbolInfo[]): vscode.TreeItem[] {
  const symbols = bases.map((info) => new SymbolItem(pkgItem, info));
  symbols.sort((a, b) => {
    if (a.info.kind !== b.info.kind) {
      return a.info.kind.localeCompare(b.info.kind);
    }
    return a.info.name.localeCompare(b.info.name);
  });
  return symbols;
}
