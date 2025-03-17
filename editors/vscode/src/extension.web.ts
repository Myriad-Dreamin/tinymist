import { ExtensionContext, window } from "vscode";
import { loadTinymistConfig } from "./config";
import { tinymistActivate, tinymistDeactivate } from "./extension.shared";
import { extensionState } from "./state";

const webActivateTable = () => [];

export async function activate(context: ExtensionContext): Promise<void> {
  extensionState.features = {
    web: true,
    lsp: false,
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
