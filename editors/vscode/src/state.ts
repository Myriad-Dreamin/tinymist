import * as vscode from "vscode";

export type ExtensionContext = vscode.ExtensionContext;

interface ExtensionState {
  features: {
    task: boolean;
    devKit: boolean;
    dragAndDrop: boolean;
    onEnter: boolean;
    preview: boolean;
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
    devKit: false,
    dragAndDrop: false,
    onEnter: false,
    preview: false,
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
