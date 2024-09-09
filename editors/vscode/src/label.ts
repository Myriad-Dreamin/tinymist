import * as vscode from "vscode";
import { tinymist } from "./lsp";
import { WorkspaceSymbol } from "vscode-languageclient";

export function labelViewActivate(context: vscode.ExtensionContext) {
  const labelViewProvider = new LabelViewProviderProvider();
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.label-view", labelViewProvider),
    // tinymist.syncLabel
    vscode.commands.registerCommand("tinymist.syncLabel", async () => {
      labelViewProvider.changeTreeDataEvent.fire(undefined);
    }),
  );
}

class LabelViewProviderProvider implements vscode.TreeDataProvider<LabelViewItem> {
  changeTreeDataEvent = new vscode.EventEmitter<LabelViewItem | undefined>();
  onDidChangeTreeData = this.changeTreeDataEvent.event;

  previousRoots?: LabelViewItem[];

  constructor() {}

  refresh(): void {}

  getTreeItem(element: LabelViewItem): vscode.TreeItem {
    return element;
  }

  async getChildren(element?: LabelViewItem): Promise<LabelViewItem[]> {
    if (element) {
      return Promise.resolve(element.children ?? []);
    }

    console.log("scan labels again");

    const labels = await tinymist.getWorkspaceLabels();
    const client = await tinymist.getClient();

    const items = labels.map((label) => {
      const loc = client.protocol2CodeConverter.asLocation(label.location);
      return new LabelViewItem(label.name, loc);
    });

    const nextRoots = makeLabelTree(items);
    if (this.previousRoots) {
      inheritCollapseState(this.previousRoots, nextRoots);
    }

    this.previousRoots = nextRoots;

    return Promise.resolve(nextRoots);
  }
}

const labelViewIcon = new vscode.ThemeIcon("symbol-constant");
const labelViewGroupIcon = new vscode.ThemeIcon("symbol-namespace");

export class LabelViewItem extends vscode.TreeItem {
  constructor(
    public label: string,
    location: vscode.Location | undefined = undefined,
    public iconPath = labelViewIcon,
    public readonly command: vscode.Command | undefined = location
      ? {
          title: "Reveal Label",
          command: "vscode.open",
          arguments: [
            location.uri,
            {
              selection: location.range,
              preview: true,
            },
          ],
        }
      : undefined,
  ) {
    super(label, vscode.TreeItemCollapsibleState.None);
  }

  childrenMap?: Map<string, LabelViewItem>;
  children?: LabelViewItem[];
  refs?: LabelViewItem[];

  contextValue = "label-view-item";
}

function makeLabelTree(labels: LabelViewItem[]): LabelViewItem[] {
  // split labels by : and any space
  const splitRegex = /[:\s]+/;

  const trie = new LabelViewItem("");

  for (const label of labels) {
    const parts = label.label.split(splitRegex);

    let currentTrie = trie;
    for (const part of parts) {
      const mp = (currentTrie.childrenMap ??= new Map<string, LabelViewItem>());
      currentTrie = mp.get(part)!;
      if (!currentTrie) {
        mp.set(part, (currentTrie = new LabelViewItem(part)));
      }
    }
    (currentTrie.refs ||= []).push(label);
  }

  const items = mergeLabelTree(trie).children!;

  assignDescription(items, "");
  return items;
}

function mergeLabelTree(trie: LabelViewItem): LabelViewItem {
  trie.children = [...(trie.refs || []), ...mergeLabelTreeChildren(trie)];
  if (trie.children.length !== 1) {
    trie.collapsibleState = vscode.TreeItemCollapsibleState.Collapsed;
    return trie;
  }

  const child = trie.children[0];
  if (!child.command) {
    child.label = trie.label + " " + child.label;
  }
  return child;
}

function mergeLabelTreeChildren(trie: LabelViewItem): LabelViewItem[] {
  if (!trie.childrenMap) {
    return [];
  }

  const items = Array.from(trie.childrenMap.values()).map(mergeLabelTree);
  items.sort((a, b) => a.label.localeCompare(b.label));

  return items;
}

function assignDescription(items: LabelViewItem[], prefix: string) {
  for (const item of items) {
    if (!item.command) {
      item.iconPath = labelViewGroupIcon;

      const desc = prefix.length > 0 ? `${prefix} ${item.label}` : item.label;
      if (desc !== item.label) {
        item.description = desc;
      }
      assignDescription(item.children ?? [], desc);
    }
  }
}

function inheritCollapseState(previousRoots: LabelViewItem[], nextRoots: LabelViewItem[]) {
  if (previousRoots.length === 0) {
    return;
  }

  const prevMap = new Map<string, LabelViewItem>();
  for (const root of previousRoots) {
    prevMap.set(root.label, root);
  }

  for (const root of nextRoots) {
    if (root.collapsibleState === vscode.TreeItemCollapsibleState.None) {
      continue;
    }

    const prev = prevMap.get(root.label);
    if (prev) {
      root.collapsibleState = prev.collapsibleState;
      inheritCollapseState(prev.children ?? [], root.children ?? []);
    }
  }
}
