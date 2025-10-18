/** biome-ignore-all lint/suspicious/noExplicitAny: type-erased */
import * as vscode from "vscode";
import type { IContext } from "../context";
import type { ExtensionContext } from "../state";
import type { EditorTool, EditorToolContext } from "../tools";
import { isTypstDocument, loadHTMLFile } from "../util";
import { handleMessage, type WebviewMessage } from "./tool/message-handler";
import { tools } from "./tool/registry";
import { ToolViewProvider } from "./tool/views";

export const FONTS_EXPORT_CONFIG_VERSION = "0.0.1";
export const USER_PACKAGE_VERSION = "0.0.1";

export interface Versioned<T> {
  version: string;
  data: T;
}

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
  const tool = getTool<T, TOptions>(toolId);

  const disposalManager = new DisposalManager();

  const dispose = () => {
    tool.dispose?.();
    disposalManager.dispose();
  };

  const toolContext: EditorToolContext<TOptions> = {
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

  // Register message handler
  disposalManager.add(
    panel.webview.onDidReceiveMessage(async (message: WebviewMessage) => {
      console.log("onDidReceiveMessage", message);
      handleMessage(message, toolContext);
    }),
  );

  // Track focused Typst document
  let focusedDocVersion = 0;
  disposalManager.add(
    vscode.window.onDidChangeTextEditorSelection(async (event) => {
      if (!isTypstDocument(event.textEditor.document)) {
        return;
      }
      focusedDocVersion++;
      toolContext.postMessage({
        type: "focusTypstDoc",
        version: focusedDocVersion,
        fsPath: event.textEditor.document.uri.fsPath,
      });
    }),
  );

  // Handle panel disposal
  disposalManager.add(panel.onDidDispose(dispose));

  let html = await loadToolHtml(tool, context);

  if (tool.transformHtml) {
    const transformed = await tool.transformHtml(html, toolContext);
    if (transformed) {
      html = transformed;
    } else {
      dispose();
      return;
    }
  }

  panel.webview.html = html;

  await tool.postLoadHtml?.(toolContext);
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

/**
 * Simple disposal manager for cleaning up resources
 */
export class DisposalManager {
  private disposables: vscode.Disposable[] = [];
  private disposed = false;

  /**
   * Add a disposable resource
   */
  add(disposable: vscode.Disposable): void {
    if (this.disposed) {
      disposable.dispose();
    } else {
      this.disposables.push(disposable);
    }
  }

  /**
   * Dispose all resources
   */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;

    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.disposables.length = 0;
  }

  /**
   * Check if disposed
   */
  get isDisposed(): boolean {
    return this.disposed;
  }
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
