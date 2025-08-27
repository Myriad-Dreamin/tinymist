/** biome-ignore-all lint/suspicious/noExplicitAny: type-erased */
import type { ExtensionContext } from "../../state";
import type * as vscode from "vscode";

export interface PostLoadHtmlContext<Options = any> {
  context: ExtensionContext;
  dispose: () => void;
  addDisposable: (disposable: vscode.Disposable) => void;
  postMessage: (message: any) => void;
  opts: Options;
}

export interface EditorTool<Options = any> {
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
