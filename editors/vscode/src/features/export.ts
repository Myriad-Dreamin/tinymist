import * as vscode from "vscode";
import { l10nMsg } from "../l10n";
import { commandExport, commandShow } from "../extension";

export type ExportKind = "Pdf" | "Html" | "Svg" | "Png" | "Markdown" | "Text" | "Query";

export interface QuickExportFormatMeta {
  label: string;
  description: string;
  exportKind: ExportKind;
  extraOpts?: any;
}

const quickExports: QuickExportFormatMeta[] = [
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

export async function commandAskAndExport(): Promise<void> {
  const picked = await vscode.window.showQuickPick(quickExports, {
    title: l10nMsg("Pick a method to export"),
  });

  if (picked === undefined) {
    return;
  }

  console.log("picked", picked);

  await commandExport(picked.exportKind, picked.extraOpts);
}

export async function commandAskAndShow(): Promise<void> {
  const picked = await vscode.window.showQuickPick(quickExports, {
    title: l10nMsg("Pick a method to export and show"),
  });

  if (picked === undefined) {
    return;
  }

  console.log("picked", picked);

  await commandShow(picked.exportKind, picked.extraOpts);
}
