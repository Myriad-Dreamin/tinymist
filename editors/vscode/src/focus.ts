import { tinymist } from "./lsp";
import * as vscode from "vscode";

export class FocusState {
  private mainTimeout: NodeJS.Timeout | undefined = undefined;
  private mainPath: string | undefined = undefined;
  private mainDoc: vscode.TextDocument | undefined = undefined;

  reset() {
    if (this.mainTimeout) {
      clearTimeout(this.mainTimeout);
    }
  }

  // Informs the server after a while. This is not done intemediately to avoid
  // the cases that the user removes the file from the editor and then opens
  // another one in a short time.
  focusMain(doc?: vscode.TextDocument, fsPath?: string) {
    if (this.mainTimeout) {
      clearTimeout(this.mainTimeout);
    }
    this.mainDoc = doc;
    if (this.mainDoc?.isClosed) {
      this.mainDoc = undefined;
    }
    this.mainPath = fsPath;
    this.mainTimeout = setTimeout(async () => {
      tinymist.executeCommand("tinymist.focusMain", [fsPath]);
    }, 100);
  }

  get path() {
    return this.mainPath;
  }
  get doc() {
    return this.mainDoc;
  }
}
