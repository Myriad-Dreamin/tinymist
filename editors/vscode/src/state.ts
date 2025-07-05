import * as vscode from "vscode";
import { PreviewPanelContext } from "./features/preview";
import { PreviewViewport } from "./lsp";

export type ExtensionContext = vscode.ExtensionContext;

interface ExtensionState {
  features: {
    web: boolean;
    lsp: boolean;
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
    focusingFile: string | undefined;
    focusingDoc: vscode.TextDocument | undefined;
    focusingPreviewPanelContext: PreviewPanelContext | undefined;
    previewViewports: Record<string, PreviewViewport>;
  };
  getFocusingFile(): string | undefined;
  getFocusingDoc(): vscode.TextDocument | undefined;
  getFocusingPreviewPanelContext(): PreviewPanelContext | undefined;
  getPreviewViewport(taskId: string): PreviewViewport | undefined;
  setPreviewViewport(taskId: string, viewport: PreviewViewport): void;
}

export const extensionState: ExtensionState = {
  features: {
    web: false,
    lsp: true,
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
    focusingFile: undefined,
    focusingDoc: undefined,
    focusingPreviewPanelContext: undefined,
    previewViewports: {},
  },
  getFocusingFile() {
    return extensionState.mut.focusingFile;
  },
  getFocusingDoc() {
    return extensionState.mut.focusingDoc;
  },
  getFocusingPreviewPanelContext() {
    return extensionState.mut.focusingPreviewPanelContext;
  },
  getPreviewViewport(taskId: string) {
    return extensionState.mut.previewViewports[taskId];
  },
  setPreviewViewport(taskId: string, viewport: PreviewViewport) {
    extensionState.mut.previewViewports[taskId] = viewport;
  },
};
