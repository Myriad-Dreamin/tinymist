import * as vscode from "vscode";

/**
 * The active editor owning *typst language document* to track.
 */
let activeEditor: vscode.TextEditor | undefined;

export class IContext {
  subscriptions: vscode.Disposable[];

  fileLevelCodelens: ICommand[];

  constructor(public context: vscode.ExtensionContext) {
    this.subscriptions = context.subscriptions;
    this.fileLevelCodelens = [];

    // Tracks the active editor owning *typst language document*.
    context.subscriptions.push(
      vscode.window.onDidChangeActiveTextEditor((editor: vscode.TextEditor | undefined) => {
        const langId = editor?.document.languageId;
        if (langId === "typst") {
          activeEditor = editor;
        } else if (editor === undefined || activeEditor?.document.isClosed) {
          activeEditor = undefined;
        }
      }),
    );
  }

  // todo: remove me
  static currentActiveEditor(): vscode.TextEditor | undefined {
    return activeEditor;
  }

  registerFileLevelCommand(command: IFileLevelCommand) {
    this.fileLevelCodelens.push(command);
    this.subscriptions.push(
      vscode.commands.registerCommand(command.command, (...args: unknown[]) =>
        command.execute(this.provideEditor({}), ...args),
      ),
    );
  }

  provideEditor(context: FileLevelContext): FileLevelContext {
    context.currentEditor = activeEditor || vscode.window.activeTextEditor;

    return context;
  }

  getCwd(context: any) {
    if (context.cwd) {
      return context.cwd;
    }

    return this.getRootForUri(context.currentEditor?.document?.uri as vscode.Uri);
  }

  getRootForUri(uri?: vscode.Uri) {
    const enclosedRoot = uri && vscode.workspace.getWorkspaceFolder(uri);
    if (enclosedRoot) {
      return enclosedRoot.uri.fsPath;
    }

    if (uri) {
      return vscode.Uri.joinPath(uri, "..").fsPath;
    }

    return undefined;
  }

  isValidEditor(currentEditor: vscode.TextEditor | undefined): currentEditor is vscode.TextEditor {
    if (!currentEditor) {
      vscode.window.showWarningMessage("No editor found for command.");
      return false;
    }

    return true;
  }

  // todo: provide it correctly.
  tinymistExec?: ICommand<ExecContext, Promise<ExecResult | undefined>>;

  showErrorMessage(message: string) {
    vscode.window.showErrorMessage(message);
  }
}

export interface FileLevelContext {
  currentEditor?: vscode.TextEditor;
}

export interface ICommand<T = unknown, R = any> {
  command: string;
  execute(context: T, ...args: unknown[]): R;
}

export type IFileLevelCommand = ICommand<FileLevelContext>;

export interface ExecContext extends FileLevelContext {
  cwd?: string;
  isTTY?: boolean;
  stdout?: (data: Buffer) => void;
  stderr?: (data: Buffer) => void;
  killer?: vscode.EventEmitter<void>;
}

export interface ExecResult {
  stdout: Buffer;
  stderr: Buffer;
  code: number;
  signal?: any;
}
