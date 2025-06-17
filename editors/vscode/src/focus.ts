import { tinymist } from "./lsp";
import * as vscode from "vscode";

export class FocusState {
  private mainTimeout: NodeJS.Timeout | undefined = undefined;
  private mainDoc: vscode.TextDocument | undefined = undefined;
  private subscribedSelection: boolean = false;
  private lastSyncPath?: string;

  get doc() {
    return this.mainDoc;
  }

  /**
   * Resets the state of the focus. This is called before the extension is
   * (re-)activated.
   */
  reset() {
    if (this.mainTimeout) {
      clearTimeout(this.mainTimeout);
    }
    this.lastSyncPath = undefined;
  }

  /**
   * Informs the server after a while. This is not done intemediately to avoid
   * the cases that the user removes the file from the editor and then opens
   * another one in a short time.
   */
  focusMain(doc?: vscode.TextDocument, editor?: vscode.TextEditor) {
    this.mainDoc = doc;
    if (this.mainDoc?.isClosed) {
      this.mainDoc = undefined;
    }

    this.subscribeChange();
    this.lspChangeSelect(doc, editor ? () => editor.selections : undefined);
  }

  /**
   * Lazily subscribes to the selection change event.
   */
  private subscribeChange() {
    if (this.subscribedSelection) {
      return;
    }
    this.subscribedSelection = true;

    vscode.window.onDidChangeTextEditorSelection((event) => {
      if (this.mainDoc && uriEquals(event.textEditor.document.uri, this.mainDoc.uri)) {
        this.lspChangeSelect(event.textEditor.document, () => event.selections);
      }
    });
  }

  private lspChangeSelect(doc?: vscode.TextDocument, select?: () => readonly vscode.Selection[]) {
    if (this.mainTimeout) {
      clearTimeout(this.mainTimeout);
    }
    this.mainTimeout = setTimeout(async () => {
      const fsPath = doc
        ? doc.isUntitled
          ? "/untitled/" + doc.uri.fsPath
          : doc.uri.fsPath
        : undefined;
      const opts = select ? { selections: await this.convertLspSelections(select()) } : undefined;

      if (this.lastSyncPath === fsPath) {
        tinymist.executeCommand("tinymist.changeSelections", [opts?.selections]);
      } else {
        tinymist.executeCommand("tinymist.focusMain", [fsPath, opts]);
      }
    }, 100);
  }

  async convertLspSelections(selections: readonly vscode.Selection[]) {
    const client = await tinymist.getClient();

    return selections.map((s) => client.code2ProtocolConverter.asRange(s));
  }
}

function uriEquals(lhs: vscode.Uri, rhs: vscode.Uri) {
  return lhs.toString(true) === rhs.toString(true);
}
