import * as vscode from "vscode";
import type { MessageHandler } from "./message-handler";
import { extensionState } from "../../state";
import { USER_PACKAGE_VERSION } from "./tools/template-gallery";
import { FONTS_EXPORT_CONFIGURE_VERSION } from "./tools/summary";

// Message type interfaces
// todo: make API typed in both sides
interface CopyToClipboardMessage {
  type: "copyToClipboard";
  content: string;
}

interface EditTextMessage {
  type: "editText";
  edit: { newText: string | Record<string, string> };
}

interface RevealPathMessage {
  type: "revealPath";
  path: string;
}

interface SaveDataToFileMessage {
  type: "saveDataToFile";
  path: string | unknown;
  data: string;
  option: vscode.SaveDialogOptions;
}

interface SaveFontsExportConfigureMessage {
  type: "saveFontsExportConfigure";
  data: unknown;
}

interface SavePackageDataMessage {
  type: "savePackageData";
  data: unknown;
}

interface InitTemplateMessage {
  type: "initTemplate";
  packageSpec: string;
}

interface StopServerProfilingMessage {
  type: "stopServerProfiling";
}

export const messageHandlers: Record<string, MessageHandler> = {
  copyToClipboard: async ({ content }: CopyToClipboardMessage) => {
    await vscode.env.clipboard.writeText(content);
  },

  editText: async ({ edit }: EditTextMessage) => {
    const activeDocument = extensionState.getFocusingDoc();
    if (!activeDocument) {
      await vscode.window.showErrorMessage("No focusing document");
      return;
    }

    const editor = vscode.window.visibleTextEditors.find(
      (editor) => editor.document === activeDocument,
    );
    if (!editor) {
      await vscode.window.showErrorMessage("No focusing editor");
      return;
    }

    // get cursor
    const selection = editor.selection;
    const selectionStart = selection.start;

    if (typeof edit.newText === "string") {
      // replace the selection with the new text
      await editor.edit((editBuilder) => {
        editBuilder.replace(selection, edit.newText as string);
      });
    } else {
      const {
        kind,
        math,
        comment,
        markup,
        code,
        string: stringContent,
        raw,
        rest,
      }: Record<string, string> = edit.newText;
      const newText = kind === "by-mode" ? rest || "" : "";

      const res = await vscode.commands.executeCommand<
        [{ mode: "math" | "markup" | "code" | "comment" | "string" | "raw" }]
      >("tinymist.interactCodeContext", {
        textDocument: {
          uri: activeDocument.uri.toString(),
        },
        query: [
          {
            kind: "modeAt",
            position: {
              line: selectionStart.line,
              character: selectionStart.character,
            },
          },
        ],
      });

      const mode = res[0].mode;

      await editor.edit((editBuilder) => {
        if (mode === "math") {
          // todo: whether to keep stupid
          // if it is before an identifier character, then add a space
          let replaceText = math || newText;
          const range = new vscode.Range(
            selectionStart.with(undefined, selectionStart.character - 1),
            selectionStart,
          );
          const before = selectionStart.character > 0 ? activeDocument.getText(range) : "";
          if (before.match(/[\p{XID_Start}\p{XID_Continue}_]/u)) {
            replaceText = ` ${math}`;
          }

          editBuilder.replace(selection, replaceText);
        } else if (mode === "markup") {
          editBuilder.replace(selection, markup || newText);
        } else if (mode === "comment") {
          editBuilder.replace(selection, comment || markup || newText);
        } else if (mode === "string") {
          editBuilder.replace(selection, stringContent || raw || newText);
        } else if (mode === "raw") {
          editBuilder.replace(selection, raw || stringContent || newText);
        } else if (mode === "code") {
          editBuilder.replace(selection, code || newText);
        } else {
          editBuilder.replace(selection, newText);
        }
      });
    }
  },

  revealPath: async ({ path }: RevealPathMessage) => {
    await vscode.commands.executeCommand("vscode.open", vscode.Uri.file(path));
    await vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(path));
  },

  saveDataToFile: async ({ path, data, option }: SaveDataToFileMessage) => {
    if (typeof path !== "string") {
      const uri = await vscode.window.showSaveDialog(option);
      path = uri?.fsPath;
    }
    if (typeof path !== "string") {
      return;
    }
    const fs = await import("node:fs/promises");
    await fs.writeFile(path, data);
  },

  saveFontsExportConfigure: async ({ data }: SaveFontsExportConfigureMessage, context) => {
    await context.context.globalState.update("fontsExportConfigure", {
      version: FONTS_EXPORT_CONFIGURE_VERSION,
      data,
    });
  },

  savePackageData: async ({ data }: SavePackageDataMessage, context) => {
    await context.context.globalState.update("userPackageData", {
      version: USER_PACKAGE_VERSION,
      data,
    });
  },

  initTemplate: async ({ packageSpec }: InitTemplateMessage, context) => {
    const initArgs = [packageSpec];
    const path = await vscode.window.showOpenDialog({
      canSelectFiles: false,
      canSelectFolders: true,
      canSelectMany: false,
      openLabel: "Select folder to initialize",
    });
    if (path === undefined) {
      return;
    }
    initArgs.push(path[0].fsPath);

    await vscode.commands.executeCommand("tinymist.initTemplate", ...initArgs);
    context.dispose();
  },

  stopServerProfiling: async (_: StopServerProfilingMessage, context) => {
    console.log("Stopping server profiling...");
    const traceDataTask = await vscode.commands.executeCommand("tinymist.stopServerProfiling");
    const traceData = await traceDataTask;

    // Check if panel is still valid before posting message
    if (context.panel.webview) {
      context.panel.webview.postMessage({ type: "traceData", data: traceData });
    }
  },
};
