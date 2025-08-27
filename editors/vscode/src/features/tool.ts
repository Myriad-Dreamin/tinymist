/** biome-ignore-all lint/suspicious/noExplicitAny: type-erased */
import * as vscode from "vscode";
import type { ExtensionContext } from "../state";
import { loadHTMLFile } from "../util";
import type { IContext } from "../context";
import type { EditorTool, PostLoadHtmlContext as EditorToolContext } from "./tools";
import { tools } from "./tools/registry";
import {
  handleMessage,
  type MessageHandlerContext,
  type WebviewMessage,
} from "./tools/message-handler";
import { DisposalManager } from "./tools/disposal-manager";

export function toolActivate(context: IContext) {
  const toolView = new ToolViewProvider();

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.tool-view", toolView),
    ...tools
      .filter((tool) => tool.command)
      .map((tool) =>
        vscode.commands.registerCommand(tool.command?.command || "", async () => {
          await createEditorToolView(context.context, tool.id);
        }),
      ),
  );
}

function getTool<T extends EditorTool<TOptions>, TOptions = any>(toolId: string): T {
  const tool = tools.find((t) => t.id === toolId);
  if (!tool) {
    throw new Error(`Tool not found: ${toolId}`);
  }
  return tool as T;
}

export async function createEditorToolView<T extends EditorTool<TOptions>, TOptions = any>(
  context: ExtensionContext,
  toolId: string,
  opts?: TOptions,
): Promise<void> {
  const tool = getTool<T, TOptions>(toolId);

  // Create and show a new WebView
  const panel = vscode.window.createWebviewPanel(
    `tinymist-${toolId}`,
    tool.title instanceof Function
      ? tool.title(opts as TOptions)
      : (tool.title ?? tool.command?.title ?? tool.id),
    {
      viewColumn: vscode.ViewColumn.Beside, // Which sides
      preserveFocus: tool.showOption?.preserveFocus ?? false,
    },
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      enableFindWidget: tool.webviewPanelOptions?.enableFindWidget ?? false,
    },
  );

  await updateEditorToolView(context, toolId, panel, opts);
}

export async function updateEditorToolView<T extends EditorTool<TOptions>, TOptions = any>(
  context: ExtensionContext,
  toolId: string,
  panel: vscode.WebviewView | vscode.WebviewPanel,
  opts?: TOptions,
): Promise<void> {
  const toolInstance = getTool<T, TOptions>(toolId);

  const disposalManager = new DisposalManager();

  const dispose = () => {
    toolInstance.dispose?.();
    disposalManager.dispose();
  };

  const messageHandlerContext: MessageHandlerContext = {
    context,
    panel,
    dispose,
    addDisposable: (disposable) => disposalManager.add(disposable),
  };

  // Register message handler
  disposalManager.add(
    panel.webview.onDidReceiveMessage(async (message: WebviewMessage) => {
      console.log("onDidReceiveMessage", message);
      handleMessage(message, messageHandlerContext);
    }),
  );

  // Handle panel disposal
  disposalManager.add(panel.onDidDispose(dispose));

  let html = await loadToolHtml(toolInstance, context);

  const postLoadHtmlContext: EditorToolContext<TOptions> = {
    context,
    opts: opts as TOptions,
    dispose,
    addDisposable: (disposable) => disposalManager.add(disposable),
    postMessage: (message) => {
      if (!disposalManager.isDisposed) {
        panel.webview.postMessage(message);
      }
    },
  };

  if (toolInstance.transformHtml) {
    const transformed = await toolInstance.transformHtml(html, postLoadHtmlContext);
    if (transformed) {
      html = transformed;
    } else {
      dispose();
      return;
    }
  }

  panel.webview.html = html;

  await toolInstance.postLoadHtml?.(postLoadHtmlContext);
}

async function loadToolHtml<TOptions>(
  tool: EditorTool<TOptions>,
  context: ExtensionContext,
): Promise<string> {
  const appDir = tool.appDir ?? "default";
  const html = await loadHTMLFile(context, `./out/editor-tools/${appDir}/index.html`);

  return html.replace(
    /`editor-tools-args:{"page": [^`]*?`/,
    `\`editor-tools-args:{"page": "${tool.id}"}\``,
  );
}

class ToolViewProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
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

export const USER_PACKAGE_VERSION = "0.0.1";

interface Versioned<T> {
  version: string;
  data: T;
}

interface PackageData {
  [ns: string]: {
    [packageName: string]: {
      isFavorite: boolean;
    };
  };
}

export function getUserPackageData(context: ExtensionContext) {
  const defaultPackageData: Versioned<PackageData> = {
    version: USER_PACKAGE_VERSION,
    data: {},
  };

  const userPackageData = context.globalState.get("userPackageData", defaultPackageData);
  if (userPackageData?.version !== USER_PACKAGE_VERSION) {
    return defaultPackageData;
  }

  return userPackageData;
}
