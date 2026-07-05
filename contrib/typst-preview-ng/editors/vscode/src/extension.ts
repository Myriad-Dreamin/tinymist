import * as vscode from "vscode";

const TINYMIST_EXTENSION_ID = "myriad-dreamin.tinymist";

type PreviewTarget = "paged" | "html";

interface TinymistPreviewer {
  htmlPath?: string;
  supportedTargets?: PreviewTarget[];
  compatibleTinymistVersion: string;
  isCompatible?(tinymistVersion: string): Promise<boolean> | boolean;
}

interface TinymistPreviewerProvider {
  providePreviewer(): Promise<TinymistPreviewer> | TinymistPreviewer;
}

export function activate(context: vscode.ExtensionContext): TinymistPreviewerProvider {
  const outputChannel = vscode.window.createOutputChannel("Typst Preview NG");
  context.subscriptions.push(outputChannel);

  return {
    providePreviewer() {
      const tinymistVersion = String(
        vscode.extensions.getExtension(TINYMIST_EXTENSION_ID)?.packageJSON.version ?? "0.0.0",
      );
      outputChannel.appendLine(`Providing worker preview client for Tinymist ${tinymistVersion}`);

      return {
        htmlPath: "previewer/index.html",
        compatibleTinymistVersion: tinymistVersion,
        supportedTargets: ["paged"],
        isCompatible(version) {
          return version === tinymistVersion;
        },
      };
    },
  };
}

export function deactivate() {}
