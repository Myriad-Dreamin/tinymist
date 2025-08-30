import * as vscode from "vscode";
import { extensionState } from "../../state";
import type { EditorToolContext } from "../../tools";
import { FONTS_EXPORT_CONFIG_VERSION, USER_PACKAGE_VERSION } from "../tool";
import { tinymist } from "../../lsp";

export interface WebviewMessage {
  type: string;
}

export type MessageHandler = (
  // biome-ignore lint/suspicious/noExplicitAny: type-erased
  message: any,
  context: EditorToolContext,
) => Promise<void> | void;

export async function handleMessage(
  message: WebviewMessage,
  context: EditorToolContext,
): Promise<boolean> {
  const handler = messageHandlers[message.type];
  if (handler) {
    try {
      await handler(message, context);
      return true;
    } catch (error) {
      console.error(`Error handling message ${message}:`, error);
      return false;
    }
  }
  return false;
}

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

interface CreateExportTaskMessage {
  type: "createExportTask";
  taskDefinition: {
    label: string;
    type: string;
    command: string;
    args?: string[];
    group?: string;
    problemMatcher?: string[];
    options?: Record<string, unknown>;
    export?: Record<string, unknown>;
  };
}

interface ExportDocumentMessage {
  type: "exportDocument";
  format: string;
  config: Record<string, unknown>;
}

interface GeneratePreviewMessage {
  type: "generatePreview";
  format: string;
  pages?: number;
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

  saveFontsExportConfigure: async ({ data }: SaveFontsExportConfigureMessage, { context }) => {
    await context.globalState.update("fontsExportConfigure", {
      version: FONTS_EXPORT_CONFIG_VERSION,
      data,
    });
  },

  savePackageData: async ({ data }: SavePackageDataMessage, { context }) => {
    await context.globalState.update("userPackageData", {
      version: USER_PACKAGE_VERSION,
      data,
    });
  },

  initTemplate: async ({ packageSpec }: InitTemplateMessage, { dispose }) => {
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
    dispose();
  },

  stopServerProfiling: async (_: StopServerProfilingMessage, { postMessage }) => {
    console.log("Stopping server profiling...");
    const traceDataTask = await vscode.commands.executeCommand("tinymist.stopServerProfiling");
    const traceData = await traceDataTask;

    postMessage({ type: "traceData", data: traceData });
  },

  createExportTask: async ({ taskDefinition }: CreateExportTaskMessage) => {
    try {
      // Get the current workspace folder
      const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
      if (!workspaceFolder) {
        await vscode.window.showErrorMessage("No workspace folder found");
        return;
      }

      // Create tasks.json in .vscode folder if it doesn't exist
      const vscodeFolderUri = vscode.Uri.joinPath(workspaceFolder.uri, ".vscode");
      const tasksFileUri = vscode.Uri.joinPath(vscodeFolderUri, "tasks.json");

      let tasksConfig: { version: string; tasks: Record<string, unknown>[] } = {
        version: "2.0.0",
        tasks: []
      };

      try {
        // Try to read existing tasks.json
        const tasksFileContent = await vscode.workspace.fs.readFile(tasksFileUri);
        tasksConfig = JSON.parse(Buffer.from(tasksFileContent).toString());
      } catch {
        // File doesn't exist, use default config
        await vscode.workspace.fs.createDirectory(vscodeFolderUri);
      }

      // Add the new task
      tasksConfig.tasks.push(taskDefinition);

      // Write back to tasks.json
      const tasksJsonContent = JSON.stringify(tasksConfig, null, 2);
      await vscode.workspace.fs.writeFile(tasksFileUri, Buffer.from(tasksJsonContent));

      await vscode.window.showInformationMessage(`Task "${taskDefinition.label}" created successfully`);
    } catch (error) {
      await vscode.window.showErrorMessage(`Failed to create task: ${error}`);
    }
  },

  exportDocument: async ({ format, config }: ExportDocumentMessage) => {
    try {
      // Get the active document
      const uri = extensionState.getFocusingFile();
      if (!uri) {
        await vscode.window.showErrorMessage("No active document found");
        return;
      }

      // Map format to export function
      const exportFunctions: Record<string, (uri: string, opts?: Record<string, unknown>) => Promise<string>> = {
        pdf: tinymist.exportPdf,
        png: tinymist.exportPng,
        svg: tinymist.exportSvg,
        html: tinymist.exportHtml,
        markdown: tinymist.exportMarkdown,
        tex: tinymist.exportTeX,
        text: tinymist.exportText,
        query: tinymist.exportQuery,
      };

      const exportFunction = exportFunctions[format];
      if (!exportFunction) {
        await vscode.window.showErrorMessage(`Unsupported export format: ${format}`);
        return;
      }

      // Execute export with configuration
      const resultPath = await exportFunction(uri, config);

      await vscode.window.showInformationMessage(`Exported successfully to: ${resultPath}`);
    } catch (error) {
      await vscode.window.showErrorMessage(`Export failed: ${error}`);
    }
  },

  generatePreview: async ({ format, pages }: GeneratePreviewMessage, { postMessage }) => {
    try {
      // Get the active document
      const activeDocument = extensionState.getFocusingDoc();
      if (!activeDocument) {
        postMessage({
          type: "previewError",
          error: "No active document found"
        });
        return;
      }

      const uri = activeDocument.uri.toString();

      // For visual formats, we'll use PNG export to generate previews
      if (format === "pdf" || format === "png" || format === "svg") {
        const exportFunction = format === "svg" ? tinymist.exportSvg : tinymist.exportPng;

        // Generate preview with appropriate options
        const previewOptions = {
          ppi: 96, // Lower resolution for preview
          page: pages ? { count: pages } : "first",
        };

        const previewPath = await exportFunction(uri, previewOptions);

        // Read the generated file and convert to base64
        const fs = await import("node:fs/promises");
        const fileContent = await fs.readFile(previewPath);
        const base64Content = fileContent.toString("base64");

        postMessage({
          type: "previewGenerated",
          format,
          content: base64Content,
          mimeType: format === "svg" ? "image/svg+xml" : "image/png"
        });
      } else {
        // For text-based formats, just return the format info
        postMessage({
          type: "previewGenerated",
          format,
          content: `Preview not available for ${format} format`,
          mimeType: "text/plain"
        });
      }
    } catch (error) {
      postMessage({
        type: "previewError",
        error: `Preview generation failed: ${error}`
      });
    }
  },
};
