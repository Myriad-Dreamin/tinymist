import type { ExtensionContext } from "../../state";
import type * as vscode from "vscode";

export interface PostLoadHtmlContext<Options = unknown> {
  context: ExtensionContext;
  panel: vscode.WebviewView | vscode.WebviewPanel;
  disposed: boolean;
  dispose: () => void;
  addDisposable: (disposable: vscode.Disposable) => void;
  postMessage: (message: unknown) => void;
  opts: Options;
}

export interface EditorTool<Options = unknown> {
  id: string;
  command?: vscode.Command;
  title?: string | ((opts: Options) => string);
  showOption?: {
    preserveFocus?: boolean;
  };
  webviewPanelOptions?: {
    enableFindWidget?: boolean;
  };
  appDir?: string;

  /**
   * The panel will be disposed if null or undefined is returned.
   */
  transformHtml?: (
    html: string,
    ctx: PostLoadHtmlContext<Options>,
  ) => Promise<string | null | undefined> | string | null | undefined;

  postLoadHtml?: (ctx: PostLoadHtmlContext<Options>) => Promise<void> | void;

  /**
   * Called when the tool is being disposed
   */
  dispose?: () => void;
}

export const defineEditorTool = <Options>(tool: EditorTool<Options>) => tool;
