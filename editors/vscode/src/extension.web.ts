import { ExtensionContext, window } from "vscode";
import { loadTinymistConfig } from "./config";
import { FeatureEntry, tinymistActivate, tinymistDeactivate } from "./extension.shared";
import { extensionState } from "./state";
import { createBrowserLanguageClient } from "./lsp.web";
import { LanguageState } from "./lsp";

const webActivateTable = (): FeatureEntry[] => [];

LanguageState.Client = createBrowserLanguageClient;

export async function activate(context: ExtensionContext): Promise<void> {
  extensionState.features = {
    web: true,
    lsp: true,
    lspSystem: false,
    export: false,
    task: false,
    wordSeparator: true,
    label: false,
    package: false,
    tool: false,
    devKit: false,
    dragAndDrop: false,
    copyAndPaste: false,
    onEnter: false,
    testing: false,
    testingDebug: false,
    preview: false,
    language: false,
    renderDocs: false,
  };

  try {
    return await tinymistActivate(context, {
      activateTable: webActivateTable,
      config: loadTinymistConfig(),
    });
  } catch (e) {
    void window.showErrorMessage(`Failed to activate tinymist: ${e}`);
    throw e;
  }
}

export async function deactivate(): Promise<void> {
  tinymistDeactivate({
    activateTable: webActivateTable,
  });
}
