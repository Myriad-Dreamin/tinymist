import * as vscode from "vscode";
import type { ExtensionContext } from "../state";
import { loadHTMLFile } from "../util";
import type { IContext } from "../context";
import type { EditorTool, PostLoadHtmlContext } from "./tools";
import { ToolRegistry } from "./tools/registry";
import {
  handleMessage,
  type MessageHandlerContext,
  type WebviewMessage,
} from "./tools/message-handler";

export function toolActivate(context: IContext) {
  const toolView = new ToolViewProvider();
  const toolRegistry = ToolRegistry.getInstance();

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.tool-view", toolView),
    ...toolRegistry.getToolsWithCommands().map((tool) =>
      vscode.commands.registerCommand(tool.command?.command || "", async () => {
        await editorTool(context.context, tool.id as EditorToolName);
      }),
    ),
  );
}

export type EditorToolName =
  | "template-gallery"
  | "tracing"
  | "profile-server"
  | "summary"
  | "font-view"
  | "symbol-view"
  | "docs"; // todo: dynamic

function getTool(toolId: string): EditorTool {
  const toolRegistry = ToolRegistry.getInstance();
  const tool = toolRegistry.getTool(toolId);
  if (!tool) {
    throw new Error(`Tool not found: ${toolId}`);
  }
  return tool;
}

export async function editorTool(
  context: ExtensionContext,
  toolId: EditorToolName,
  opts?: unknown,
) {
  const tool = getTool(toolId);

  // Create and show a new WebView
  const panel = vscode.window.createWebviewPanel(
    `tinymist-${toolId}`,
    tool.title instanceof Function
      ? tool.title(opts)
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

  await editorToolAt(context, toolId, panel, opts);
}

export async function editorToolAt(
  context: ExtensionContext,
  toolId: EditorToolName,
  panel: vscode.WebviewView | vscode.WebviewPanel,
  opts?: unknown,
) {
  const tool = getTool(toolId);

  const disposes: vscode.Disposable[] = [];
  let disposed = false;

  const dispose = () => {
    if (disposed) return;
    disposed = true;

    for (const d of disposes) {
      d.dispose();
    }

    // Call tool-specific dispose if available
    if (tool.dispose) {
      tool.dispose();
    }

    // if has dispose method
    if ("dispose" in panel) {
      panel.dispose();
    }
  };

  const registerDisposable = (disposable: vscode.Disposable) => {
    if (!disposed) {
      disposes.push(disposable);
    } else {
      disposable.dispose();
    }
  };

  const messageHandlerContext: MessageHandlerContext = {
    context,
    panel,
    dispose,
    registerDisposable,
  };

  const messageDisposable = panel.webview.onDidReceiveMessage(async (message: WebviewMessage) => {
    console.log("onDidReceiveMessage", message);
    handleMessage(message, messageHandlerContext);
  });
  registerDisposable(messageDisposable);

  panel.onDidDispose(() => {
    dispose();
  });

  let html = await loadToolHtml(tool, context);

  const postLoadHtmlContext: PostLoadHtmlContext = {
    context,
    panel,
    disposed,
    dispose,
    addDisposable: registerDisposable,
    opts,
    postMessage: (message) => {
      if (!disposed) {
        panel.webview.postMessage(message);
      }
    },
  };

  if (tool.transformHtml) {
    const transformed = await tool.transformHtml(html, postLoadHtmlContext);
    if (transformed) {
      html = transformed;
    } else {
      dispose();
      return;
    }
  }

  panel.webview.html = html;

  await tool.postLoadHtml?.(postLoadHtmlContext);
}

async function loadToolHtml(tool: EditorTool, context: ExtensionContext): Promise<string> {
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
    const toolRegistry = ToolRegistry.getInstance();
    return Promise.resolve(
      toolRegistry
        .getToolsWithCommands()
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
