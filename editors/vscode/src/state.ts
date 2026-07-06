import * as vscode from "vscode";
import { PreviewPanelContext } from "./features/preview";
import { FocusState } from "./focus";

export type ExtensionContext = vscode.ExtensionContext;

interface ExtensionState {
  features: {
    web: boolean;
    lsp: boolean;
    lspSystem: boolean;
    export: boolean;
    task: boolean;
    devKit: boolean;
    wordSeparator: boolean;
    dragAndDrop: boolean;
    copyAndPaste: boolean;
    label: boolean;
    package: boolean;
    tool: boolean;
    onEnter: boolean;
    preview: boolean;
    language: boolean;
    testing: boolean;
    testingDebug: boolean;
    renderDocs: boolean;
  };
  mut: {
    focusing: FocusState;
    focusingPreviewPanelContext: PreviewPanelContext | undefined;
    serverHealthWarningShown: boolean;
    serverReady: boolean;
  };
  getFocusingFile(): string | undefined;
  getFocusingDoc(): vscode.TextDocument | undefined;
  getFocusingPreviewPanelContext(): PreviewPanelContext | undefined;
}

export const extensionState: ExtensionState = {
  features: {
    web: false,
    lsp: true,
    lspSystem: true,
    export: true,
    testingDebug: true,
    task: true,
    wordSeparator: true,
    label: true,
    package: true,
    tool: true,
    devKit: false,
    dragAndDrop: false,
    copyAndPaste: false,
    onEnter: false,
    preview: false,
    language: true,
    testing: true,
    renderDocs: false,
  },
  mut: {
    focusing: new FocusState(),
    focusingPreviewPanelContext: undefined,
    serverHealthWarningShown: false,
    serverReady: false,
  },
  getFocusingFile() {
    const doc = extensionState.getFocusingDoc();
    if (!doc) {
      return undefined;
    }

    return doc.isUntitled ? "/untitled/" + doc.uri.fsPath : doc.uri.fsPath;
  },
  getFocusingDoc() {
    const doc = extensionState.mut.focusing.doc;
    return doc?.isClosed === false ? doc : undefined;
  },
  getFocusingPreviewPanelContext() {
    return extensionState.mut.focusingPreviewPanelContext;
  },
};
