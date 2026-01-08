import * as vscode from "vscode";
import { commands } from "vscode";
import type {
  ExportActionOpts,
  ExportOpts,
  ExportPdfOpts,
  ExportPngOpts,
  ExportSvgOpts,
  ExportTypliteOpts,
} from "../cmd.export";
import type { IContext } from "../context";
import { l10nMsg } from "../l10n";
import { type ExportResponse, tinymist } from "../lsp";

/// These are names of the export functions in the LSP client, e.g. `exportPdf`, `exportHtml`.
export type ExportKind = "Pdf" | "Html" | "Svg" | "Png" | "Markdown" | "TeX" | "Text" | "Query";

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
  extraOpts?: ExportOpts;
  selectPages?: true | "merged";
}

export const quickExports: QuickExportFormatMeta[] = [
  {
    label: "PDF",
    description: l10nMsg("Export as PDF"),
    exportKind: "Pdf",
  },
  {
    label: l10nMsg("PDF (Specific Pages)"),
    description: l10nMsg("Export as PDF with specified pages"),
    exportKind: "Pdf",
    selectPages: true,
  },
  {
    label: l10nMsg("PNG (Merged)"),
    description: l10nMsg("Export as a single PNG by merging pages"),
    exportKind: "Png",
    selectPages: "merged",
  },
  {
    label: l10nMsg("PNG (Specific Pages)"),
    description: l10nMsg("Export the specified pages as multiple PNGs"),
    exportKind: "Png",
    selectPages: true,
  },
  {
    label: l10nMsg("SVG (Merged)"),
    description: l10nMsg("Export as a single SVG by merging pages"),
    exportKind: "Svg",
    selectPages: "merged",
  },
  {
    label: l10nMsg("SVG (Specific Pages)"),
    description: l10nMsg("Export the specified pages as multiple SVGs"),
    exportKind: "Svg",
    selectPages: true,
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
    label: "TeX",
    description: l10nMsg("Export as TeX"),
    exportKind: "TeX",
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
  // {
  //   label: l10nMsg("PNG (Task)"),
  //   description: l10nMsg("Export as PNG (and update tasks.json)"),
  //   exportKind: "Png",
  // },
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

  picked.extraOpts ??= {};

  if (picked.exportKind === "TeX") {
    const processor = await vscode.window.showInputBox({
      title: l10nMsg("TeX processor"),
      placeHolder: l10nMsg(
        "A typst file help export to TeX, e.g. `/ieee-tex.typ` or `@local/ieee-tex:0.1.0`",
      ),
      prompt: l10nMsg(
        "Hint: you can create and find local packages in the sidebar. See https://github.com/Myriad-Dreamin/tinymist/tree/bc15eb55cee9f9b048aafd5f22472894961a1f51/editors/vscode/e2e-workspaces/ieee-paper for more information.",
      ),
    });

    if (processor) {
      (picked.extraOpts as ExportTypliteOpts).processor = processor;
    }
  }

  if (!(await askPageSelection(picked))) {
    return; // cancelled
  }

  return cb(picked);
}

/** returns false if export cancelled */
export async function askPageSelection(picked: QuickExportFormatMeta) {
  const selectPages = picked.selectPages;
  if (!selectPages) {
    return true; // no need to select pages
  }

  picked.extraOpts ??= {};
  if (selectPages === "merged") {
    (picked.extraOpts as ExportPngOpts | ExportSvgOpts).merge = {};
    return true;
  }

  const pages = await vscode.window.showInputBox({
    title: l10nMsg("Pages to export"),
    value: "1",
    placeHolder: l10nMsg("e.g. `1-3,5,7-9`, leave empty for all pages"),
    prompt: l10nMsg("Specify the pages you want to export"),
    validateInput: validatePageRanges,
  });
  if (pages === undefined) {
    return false; // cancelled
  }

  const pageRanges = pages.split(",");
  (picked.extraOpts as ExportPdfOpts | ExportPngOpts | ExportSvgOpts).pages = pageRanges;

  if (
    (picked.exportKind === "Png" || picked.exportKind === "Svg") &&
    !(pageRanges.length === 1 && !pageRanges[0].includes("-"))
  ) {
    // multiple pages, ask for page number template
    // if only one page without range, no need for page number template
    const pageNumberTemplate = await vscode.window.showInputBox({
      title: "Page Number Template",
      placeHolder: l10nMsg("e.g., `page-{0p}-of-{t}.png`"),
      prompt: l10nMsg(
        "A page number template must be present if the source document renders to multiple pages. Use `{p}` for page numbers, `{0p}` for zero padded page numbers and `{t}` for page count.\nLeave empty for default naming scheme.",
      ),
    });
    if (pageNumberTemplate === undefined) {
      return false; // cancelled
    }

    if (pageNumberTemplate.length > 0) {
      // only set if not empty
      (picked.extraOpts as ExportPngOpts | ExportSvgOpts).pageNumberTemplate = pageNumberTemplate;
    }
  }

  return true;
}

export async function commandAskAndExport(): Promise<ExportResponse | null> {
  return (
    (await askAndRun(l10nMsg("Pick a method to export"), (picked) => {
      return commandExport(picked.exportKind, picked.extraOpts);
    })) ?? null
  );
}

export async function commandAskAndShow(): Promise<void> {
  return await askAndRun(l10nMsg("Pick a method to export and show"), (picked) => {
    return commandShow(picked.exportKind, picked.extraOpts);
  });
}

export async function commandExport(
  kind: ExportKind,
  opts?: ExportOpts,
  actionOpts?: ExportActionOpts,
): Promise<ExportResponse | null> {
  const uri = vscode.window.activeTextEditor?.document.uri.fsPath;
  if (!uri) {
    return null;
  }

  return await tinymist[`export${kind}`](uri, opts, actionOpts);
}

/**
 * Implements the functionality for the 'Show PDF' button shown in the editor title
 * if a `.typ` file is opened.
 */
export async function commandShow(kind: ExportKind, extraOpts?: ExportOpts): Promise<void> {
  const activeEditor = vscode.window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const actionOpts: ExportActionOpts = {};

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
    actionOpts.open = true;
  }

  // Gets the template from the convertExtension configuration
  const language = activeEditor.document.languageId;
  if (language === "markdown") {
    const convertExtension = conf.get("convertExtension");
    if (convertExtension && convertExtension instanceof Array) {
      const convertExtensionItem = convertExtension.find((item: any) => item.language === language);
      if (convertExtensionItem && convertExtensionItem.processor) {
        extraOpts ||= {};
        // todo: processor type
        (extraOpts as ExportPdfOpts).processor = convertExtensionItem.processor;
      }
    }
  }

  // only creates pdf if it does not exist yet
  const exportResponse = await commandExport(kind, extraOpts, actionOpts);
  if (!exportResponse || "message" in exportResponse) {
    // show error message
    await vscode.window.showErrorMessage(`Failed to export ${kind}: ${exportResponse?.message}`);
    return;
  }

  const showRes = await showExportFileIn(exportResponse, openIn);
  if (!showRes) {
    await vscode.window.showErrorMessage(`Failed to export ${kind}: no path in response`);
  }
}

async function showExportFileIn(exportResponse: ExportResponse, openIn: string): Promise<boolean> {
  // PDF export is not paged. The response should be a simple object.
  // For other formats, we just open the first page.
  const exportPath =
    "items" in exportResponse ? exportResponse.items[0]?.path : exportResponse.path;
  if (!exportPath) {
    return false;
  }

  switch (openIn) {
    case "systemDefault":
      break;
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
    default:
      vscode.window.showWarningMessage(
        `Unknown value of "tinymist.showExportFileIn", expected "systemDefault" or "editorTab", got "${openIn}"`,
      );
  }

  return true;
}

function validatePageRanges(value: string): string | undefined {
  if (!value.trim()) {
    return; // Allow empty input
  }
  const parts = value
    .split(",")
    .map((p) => p.trim())
    .filter((p) => p);
  for (const part of parts) {
    const rangeParts = part.split("-").map((s) => s.trim());
    if (rangeParts.length > 2) {
      return l10nMsg("Invalid page range format: {range}", { range: part });
    }
    if (rangeParts.length === 1) {
      // Single page
      const num = parseInt(rangeParts[0], 10);
      if (Number.isNaN(num) || num <= 0) {
        return l10nMsg("Invalid page number: {page}", { page: part });
      }
    } else {
      // Range
      const [startStr, endStr] = rangeParts;
      let startNum: number | undefined;
      let endNum: number | undefined;
      if (startStr) {
        startNum = parseInt(startStr, 10);
        if (Number.isNaN(startNum) || startNum <= 0) {
          return l10nMsg("Invalid page range: {range}", { range: part });
        }
      }
      if (endStr) {
        endNum = parseInt(endStr, 10);
        if (Number.isNaN(endNum) || endNum <= 0) {
          return l10nMsg("Invalid page range: {range}", { range: part });
        }
      }
      if (startNum !== undefined && endNum !== undefined && startNum > endNum) {
        return l10nMsg("Invalid page range: {range}", { range: part });
      }
      // If both start and end are empty, invalid
      if (!startStr && !endStr) {
        return l10nMsg("Invalid page range: {range}", { range: part });
      }
    }
  }
  return;
}
