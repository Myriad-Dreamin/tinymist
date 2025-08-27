/** biome-ignore-all lint/suspicious/noExplicitAny: type-erased */
import type * as vscode from "vscode";
import type { ExtensionContext } from "../../state";

export interface EditorToolContext<Options = any> {
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
    ctx: EditorToolContext<Options>,
  ) => Promise<string | null | undefined> | string | null | undefined;

  postLoadHtml?: (ctx: EditorToolContext<Options>) => Promise<void> | void;

  /**
   * Called when the tool is being disposed
   */
  dispose?: () => void;
}

export const defineEditorTool = <Options>(tool: EditorTool<Options>) => tool;
