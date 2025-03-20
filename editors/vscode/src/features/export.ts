import * as vscode from "vscode";
import { l10nMsg } from "../l10n";
import { tinymist } from "../lsp";
import { IContext } from "../context";
import { commands } from "vscode";

export type ExportKind = "Pdf" | "Html" | "Svg" | "Png" | "Markdown" | "Text" | "Query";

export function exportActivate(context: IContext) {
  context.subscriptions.push(
    commands.registerCommand("tinymist.exportCurrentPdf", () => commandExport("Pdf")),
    commands.registerCommand("tinymist.export", commandExport),
    commands.registerCommand("tinymist.exportCurrentFile", commandAskAndExport),
    commands.registerCommand("tinymist.showPdf", () => commandShow("Pdf")),
    commands.registerCommand("tinymist.exportCurrentFileAndShow", commandAskAndShow),
  );
}

export interface QuickExportFormatMeta {
  label: string;
  description: string;
  exportKind: ExportKind;
  extraOpts?: any;
}

export const quickExports: QuickExportFormatMeta[] = [
  {
    label: "PDF",
    description: l10nMsg("Export as PDF"),
    exportKind: "Pdf",
  },
  {
    label: l10nMsg("PNG (Merged)"),
    description: l10nMsg("Export as a single PNG by merging pages"),
    exportKind: "Png",
    extraOpts: { page: { merged: { gap: "0pt" } } },
  },
  {
    label: l10nMsg("SVG (Merged)"),
    description: l10nMsg("Export as a single SVG by merging pages"),
    exportKind: "Svg",
    extraOpts: { page: { merged: { gap: "0pt" } } },
  },
  {
    label: "HTML",
    description: l10nMsg("Export as HTML"),
    exportKind: "Html",
  },
  {
    label: "Markdown",
    description: l10nMsg("Export as Markdown"),
    exportKind: "Markdown",
  },
  {
    label: "Text",
    description: l10nMsg("Export as Text"),
    exportKind: "Text",
  },
  // {
  //   label: "Query (JSON)",
  //   description: l10nMsg("Query current document and export the result as a JSON file"),
  //   exportKind: "Query",
  // },
  // {
  //   label: "Query (YAML)",
  //   description: l10nMsg("Query current document and export the result as a YAML file"),
  //   exportKind: "Query",
  // },
  // {
  //   label: "Query (Task)",
  //   description: l10nMsg("Query current document and export the result as a file. We will ask a few questions and update the tasks.json file for you."),
  //   exportKind: "Query",
  // },
  {
    label: l10nMsg("PNG (First Page)"),
    description: l10nMsg("Export the first page as a single PNG"),
    exportKind: "Png",
  },
  // {
  //   label: l10nMsg("PNG (Task)"),
  //   description: l10nMsg("Export as PNG (and update tasks.json)"),
  //   exportKind: "Png",
  // },
  {
    label: l10nMsg("SVG (First Page)"),
    description: l10nMsg("Export the first page as a single SVG"),
    exportKind: "Svg",
  },
  // {
  //   label: l10nMsg("SVG (Task)"),
  //   description: l10nMsg("Export as SVG (and update tasks.json)"),
  //   exportKind: "Svg",
  // },
];

async function askAndRun<T>(
  title: string,
  cb: (meta: QuickExportFormatMeta) => T,
): Promise<T | undefined> {
  const picked = await vscode.window.showQuickPick(quickExports, { title });

  if (picked === undefined) {
    return;
  }
}

export async function commandAskAndExport(): Promise<string | undefined> {
  return await askAndRun(l10nMsg("Pick a method to export"), (picked) => {
    return commandExport(picked.exportKind, picked.extraOpts);
  });
}

export async function commandAskAndShow(): Promise<void> {
  return await askAndRun(l10nMsg("Pick a method to export and show"), (picked) => {
    return commandShow(picked.exportKind, picked.extraOpts);
  });
}

export async function commandExport(kind: ExportKind, opts?: any): Promise<string | undefined> {
  const uri = vscode.window.activeTextEditor?.document.uri.fsPath;
  if (!uri) {
    return;
  }

  return (await tinymist[`export${kind}`](uri, opts)) || undefined;
}

/**
 * Implements the functionality for the 'Show PDF' button shown in the editor title
 * if a `.typ` file is opened.
 */
export async function commandShow(kind: ExportKind, extraOpts?: any): Promise<void> {
  const activeEditor = vscode.window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const conf = vscode.workspace.getConfiguration("tinymist");
  const openIn: string = conf.get("showExportFileIn") || "editorTab";

  // Telling the language server to open the file instead of using
  // ```
  // vscode.env.openExternal(exportUri);
  // ```
  // , which is buggy.
  //
  // See https://github.com/Myriad-Dreamin/tinymist/issues/837
  // Also see https://github.com/microsoft/vscode/issues/85930
  const openBySystemDefault = openIn === "systemDefault";
  if (openBySystemDefault) {
    extraOpts = extraOpts || {};
    extraOpts.open = true;
  }

  // only create pdf if it does not exist yet
  const exportPath = await commandExport(kind, extraOpts);

  if (exportPath === undefined) {
    // show error message
    await vscode.window.showErrorMessage(`Failed to export ${kind}`);
    return;
  }

  switch (openIn) {
    case "systemDefault":
      break;
    default:
      vscode.window.showWarningMessage(
        `Unknown value of "tinymist.showExportFileIn", expected "systemDefault" or "editorTab", got "${openIn}"`,
      );
    // fall through
    case "editorTab": {
      // find and replace exportUri
      const exportUri = vscode.Uri.file(exportPath);
      const uriToFind = exportUri.toString();
      findTab: for (const editor of vscode.window.tabGroups.all) {
        for (const tab of editor.tabs) {
          if ((tab.input as any)?.uri?.toString() === uriToFind) {
            await vscode.window.tabGroups.close(tab, true);
            break findTab;
          }
        }
      }

      // here we can be sure that the pdf exists
      await commands.executeCommand("vscode.open", exportUri, {
        viewColumn: vscode.ViewColumn.Beside,
        preserveFocus: true,
      } as vscode.TextDocumentShowOptions);
      break;
    }
  }
}
