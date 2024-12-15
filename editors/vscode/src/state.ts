import * as vscode from "vscode";

export type ExtensionContext = vscode.ExtensionContext;

interface ExtensionState {
  features: {
    task: boolean;
    devKit: boolean;
    wordSeparator: boolean;
    dragAndDrop: boolean;
    onEnter: boolean;
    preview: boolean;
    renderDocs: boolean;
  };
  mut: {
    focusingFile: string | undefined;
    focusingDoc: vscode.TextDocument | undefined;
  };
  getFocusingFile(): string | undefined;
  getFocusingDoc(): vscode.TextDocument | undefined;
}

export const extensionState: ExtensionState = {
  features: {
    task: true,
    wordSeparator: true,
    devKit: false,
    dragAndDrop: false,
    onEnter: false,
    preview: false,
    renderDocs: false,
  },
  mut: {
    focusingFile: undefined,
    focusingDoc: undefined,
  },
  getFocusingFile() {
    return extensionState.mut.focusingFile;
  },
  getFocusingDoc() {
    return extensionState.mut.focusingDoc;
  },
};
