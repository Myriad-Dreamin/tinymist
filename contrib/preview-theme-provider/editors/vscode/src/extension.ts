import * as vscode from "vscode";

const TINYMIST_EXTENSION_ID = "myriad-dreamin.tinymist";

export async function activate(_context: vscode.ExtensionContext) {
  return {
    provideTheme() {
      const tinymistVersion = String(
        vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
      );
      return {
        htmlPath: "theme/index.html",
        compatibleTinymistVersion: tinymistVersion,
      };
    },
  };
}

export function deactivate() {}
