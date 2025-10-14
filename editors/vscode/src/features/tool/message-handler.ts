import * as vscode from "vscode";
import { extensionState } from "../../state";
import type { EditorToolContext } from "../../tools";
import { type ExportArgs, type ExportFormat, exportOps, provideFormats } from "../tasks.export";
import { FONTS_EXPORT_CONFIG_VERSION, USER_PACKAGE_VERSION } from "../tool";

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
  console.warn(`No handler for message type: ${message.type}`);
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

interface ExportDocumentMessage {
  type: "exportDocument";
  format: ExportFormat;
  extraArgs: ExportArgs;
}

interface GeneratePreviewMessage {
  type: "generatePreview";
  format: ExportFormat;
  extraArgs: ExportArgs;
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

  exportDocument: async ({ format, extraArgs }: ExportDocumentMessage) => {
    try {
      const ops = exportOps(extraArgs);
      const formatProvider = provideFormats(extraArgs);

      // Get the active document
      const uri = ops.resolveInputPath();
      if (!uri) {
        await vscode.window.showErrorMessage("No active document found");
        return;
      }

      const provider = formatProvider[format];
      if (!provider) {
        await vscode.window.showErrorMessage(`Unsupported export format: ${format}`);
        return;
      }

      // Execute export with configuration (file export by default)
      const result = await provider.export(uri, provider.opts());

      // Handle the response based on the new OnExportResponse format
      if (!result) {
        await vscode.window.showErrorMessage(`Export failed`);
        return;
      } else if ("path" in result) {
        // Success case - Single
        if (result.path) {
          await vscode.window.showInformationMessage(`Exported successfully to: ${result.path}`);
        } else {
          await vscode.window.showInformationMessage("Export completed");
        }
      } else {
        // Multiple files
        const paths = result.items.map((item) => item.path).filter(Boolean);
        if (paths.length > 0) {
          await vscode.window.showInformationMessage(
            `Exported successfully to:\n${paths.join("\n")}`,
          );
        } else {
          await vscode.window.showInformationMessage("Export completed");
        }
      }
    } catch (error) {
      await vscode.window.showErrorMessage(`Export failed: ${error}`);
    }
  },

  generatePreview: async ({ format, extraArgs }: GeneratePreviewMessage, { postMessage }) => {
    console.log(`Generating preview for format=${format}, extraArgs=`, extraArgs);
    try {
      const ops = exportOps(extraArgs);
      const formatProvider = provideFormats(extraArgs);

      // await vscode.window.showInformationMessage(`Active `)

      // Get the active document
      const uri = ops.resolveInputPath();
      if (!uri) {
        await vscode.window.showErrorMessage("No active document found");
        return;
      }

      // Use PNG for both PDF and PNG preview
      const actualFormat = format === "pdf" ? "png" : format;

      const provider = formatProvider[actualFormat];
      if (!provider) {
        await vscode.window.showErrorMessage(`Unsupported export format: ${format}`);
        return;
      }

      // Execute export with configuration (file export by default)
      const response = await provider.export(uri, provider.opts(), { write: false });
      console.log("Preview generation response:", response);
      if (!response) {
        await vscode.window.showErrorMessage("Failed to generate preview data");
        return;
      }

      // Handle error case
      if (!response) {
        postMessage({
          type: "previewError",
          error: "failed to generate preview",
        });
        return;
      }

      console.log(`Generating preview for format=${format}, extraArgs=${extraArgs}, uri=${uri}`);
      // For visual formats, generate PNG/SVG previews
      if (format === "pdf" || format === "png" || format === "svg") {
        // Extract base64 data from the response
        const renderedPages = "data" in response ? [{ page: 1, ...response }] : response.items;

        // Determine MIME type
        const mimeType = actualFormat === "svg" ? "image/svg+xml" : "image/png";

        // Multiple pages
        postMessage({
          type: "previewGenerated",
          format,
          pages: renderedPages.map((page) => ({
            pageNumber: page.page,
            imageData: `data:${mimeType};base64,${page.data}`,
          })),
        });
      } else {
        postMessage({
          type: "previewGenerated",
          format,
          text: "data" in response ? response.data : response.items[0].data,
        });
      }
    } catch (error) {
      postMessage({
        type: "previewError",
        error: `Preview generation failed: ${error}`,
      });
    }
  },
};
