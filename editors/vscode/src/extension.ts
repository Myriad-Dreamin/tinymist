import {
  type ExtensionContext,
  workspace,
  window,
  commands,
  ViewColumn,
  Uri,
  TextEditor,
} from "vscode";
import * as vscode from "vscode";
import * as path from "path";

import { loadTinymistConfig } from "./config";
import { IContext } from "./context";
import { getUserPackageData } from "./features/tool";
import { SymbolViewProvider } from "./features/tool.symbol-view";
import { LanguageState, tinymist } from "./lsp";
import { commandCreateLocalPackage, commandOpenLocalPackage } from "./package-manager";
import { extensionState } from "./state";
import { triggerStatusBar } from "./ui-extends";
import { activeTypstEditor } from "./util";
import { LanguageClient } from "vscode-languageclient/node";

import { setIsTinymist as previewSetIsTinymist } from "./features/preview-compat";
import { previewActivate, previewDeactivate } from "./features/preview";
import { taskActivate } from "./features/tasks";
import { devKitActivate } from "./features/dev-kit";
import { labelActivate } from "./features/label";
import { packageActivate } from "./features/package";
import { toolActivate } from "./features/tool";
import { copyAndPasteActivate, dragAndDropActivate } from "./features/drop-paste";
import { testingActivate } from "./features/testing";
import { testingDebugActivate } from "./features/testing/debug";
import { FeatureEntry, tinymistActivate, tinymistDeactivate } from "./extension.shared";
import { commandShow, exportActivate } from "./features/export";

LanguageState.Client = LanguageClient;

const systemActivateTable = (): FeatureEntry[] => [
  [extensionState.features.label, labelActivate],
  [extensionState.features.package, packageActivate],
  [extensionState.features.tool, toolActivate],
  [extensionState.features.dragAndDrop, dragAndDropActivate],
  [extensionState.features.copyAndPaste, copyAndPasteActivate],
  [extensionState.features.export, exportActivate],
  [extensionState.features.task, taskActivate],
  [extensionState.features.testing, testingActivate],
  [extensionState.features.testingDebug, testingDebugActivate],
  [extensionState.features.devKit, devKitActivate],
  [extensionState.features.preview, previewActivateInTinymist, previewDeactivate],
  [extensionState.features.language, languageActivate],
];

export async function activate(context: ExtensionContext): Promise<void> {
  try {
    return await tinymistActivate(context, {
      activateTable: systemActivateTable,
      config: loadTinymistConfig(),
    });
  } catch (e) {
    void window.showErrorMessage(`Failed to activate tinymist: ${e}`);
    throw e;
  }
}

export async function deactivate(): Promise<void> {
  tinymistDeactivate({
    activateTable: systemActivateTable,
  });
}

function previewActivateInTinymist(context: IContext) {
  const typstPreviewExtension = vscode.extensions.getExtension("mgt19937.typst-preview");
  if (typstPreviewExtension) {
    void vscode.window.showWarningMessage(
      "Tinymist Says:\n\nTypst Preview extension is already integrated into Tinymist. Please disable Typst Preview extension to avoid conflicts.",
    );
  }

  // Tests compat-mode preview extension
  // previewActivate(context, true);

  // Runs Integrated preview extension
  previewSetIsTinymist();
  previewActivate(context.context, false);
}

async function languageActivate(context: IContext) {
  const client = tinymist.client;
  if (!client) {
    console.warn("activating language feature without starting the tinymist language server");
    return;
  }

  // Watch all non typst files.
  // todo: more general ways to do this.
  const isInterestingNonTypst = (doc: vscode.TextDocument) => {
    return (
      doc.languageId !== "typst" && (doc.uri.scheme === "file" || doc.uri.scheme === "untitled")
    );
  };
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((doc: vscode.TextDocument) => {
      if (!isInterestingNonTypst(doc)) {
        return;
      }
      client?.sendNotification("textDocument/didOpen", {
        textDocument: client.code2ProtocolConverter.asTextDocumentItem(doc),
      });
    }),
    vscode.workspace.onDidChangeTextDocument((e: vscode.TextDocumentChangeEvent) => {
      const doc = e.document;
      if (!isInterestingNonTypst(doc) || !client) {
        return;
      }
      const contentChanges = [];
      for (const change of e.contentChanges) {
        contentChanges.push({
          range: client.code2ProtocolConverter.asRange(change.range),
          rangeLength: change.rangeLength,
          text: change.text,
        });
      }
      client.sendNotification("textDocument/didChange", {
        textDocument: client.code2ProtocolConverter.asVersionedTextDocumentIdentifier(doc),
        contentChanges,
      });
    }),
    vscode.workspace.onDidCloseTextDocument((doc: vscode.TextDocument) => {
      if (!isInterestingNonTypst(doc)) {
        return;
      }
      client?.sendNotification("textDocument/didClose", {
        textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(doc),
      });
    }),
  );
  for (const doc of vscode.workspace.textDocuments) {
    if (!isInterestingNonTypst(doc)) {
      continue;
    }

    client.sendNotification("textDocument/didOpen", {
      textDocument: client.code2ProtocolConverter.asTextDocumentItem(doc),
    });
  }

  // Find first document to focus
  const editor = window.activeTextEditor;
  if (editor?.document.languageId === "typst" && editor.document.uri.fsPath) {
    commandActivateDoc(editor.document);
  } else {
    window.visibleTextEditors.forEach((editor) => {
      if (editor.document.languageId === "typst" && editor.document.uri.fsPath) {
        commandActivateDoc(editor.document);
      }
    });
  }

  context.subscriptions.push(
    window.onDidChangeActiveTextEditor((editor: TextEditor | undefined) => {
      if (editor?.document.isUntitled) {
        return;
      }
      const langId = editor?.document.languageId;
      // todo: plaintext detection
      // if (langId === "plaintext") {
      //     console.log("plaintext", langId, editor?.document.uri.fsPath);
      // }
      if (langId !== "typst") {
        // console.log("not typst", langId, editor?.document.uri.fsPath);
        return commandActivateDoc(undefined);
      }
      return commandActivateDoc(editor?.document);
    }),
  );
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((doc: vscode.TextDocument) => {
      if (doc.isUntitled && window.activeTextEditor?.document === doc) {
        if (doc.languageId === "typst") {
          return commandActivateDocPath(doc, "/untitled/" + doc.uri.fsPath);
        } else {
          return commandActivateDoc(undefined);
        }
      }
    }),
  );
  context.subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((doc: vscode.TextDocument) => {
      if (extensionState.mut.focusingDoc === doc) {
        extensionState.mut.focusingDoc = undefined;
        commandActivateDoc(undefined);
      }
    }),
  );

  const initTemplateCommand =
    (inPlace: boolean) =>
    (...args: string[]) =>
      initTemplate(context.context, inPlace, ...args);

  // prettier-ignore
  context.subscriptions.push(
    commands.registerCommand("tinymist.openInternal", openInternal),
    commands.registerCommand("tinymist.openExternal", openExternal),

    commands.registerCommand("tinymist.getCurrentDocumentMetrics", commandGetCurrentDocumentMetrics),
    commands.registerCommand("tinymist.clearCache", commandClearCache),
    commands.registerCommand("tinymist.runCodeLens", commandRunCodeLens),
    commands.registerCommand("tinymist.copyAnsiHighlight", commandCopyAnsiHighlight),

    commands.registerCommand("tinymist.pinMainToCurrent", () => commandPinMain(true)),
    commands.registerCommand("tinymist.unpinMain", () => commandPinMain(false)),
    commands.registerCommand("typst-lsp.pinMainToCurrent", () => commandPinMain(true)),
    commands.registerCommand("typst-lsp.unpinMain", () => commandPinMain(false)),

    commands.registerCommand("tinymist.initTemplate", initTemplateCommand(false)),
    commands.registerCommand("tinymist.initTemplateInPlace", initTemplateCommand(true)),

    commands.registerCommand("tinymist.createLocalPackage", commandCreateLocalPackage),
    commands.registerCommand("tinymist.openLocalPackage", commandOpenLocalPackage),

    // We would like to define it at the server side, but it is not possible for now.
    // https://github.com/microsoft/language-server-protocol/issues/1117
    commands.registerCommand("tinymist.triggerSuggestAndParameterHints", triggerSuggestAndParameterHints),
  );
  // context.subscriptions.push
  const provider = new SymbolViewProvider(context.context);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(SymbolViewProvider.Name, provider),
  );
}

async function openInternal(target: string): Promise<void> {
  const uri = Uri.parse(target);
  await commands.executeCommand("vscode.open", uri, ViewColumn.Beside);
}

async function openExternal(target: string): Promise<void> {
  const uri = Uri.parse(target);
  await vscode.env.openExternal(uri);
}

async function commandGetCurrentDocumentMetrics(): Promise<any> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const fsPath = activeEditor.document.uri.fsPath;

  const res = await tinymist.executeCommand<string | null>(`tinymist.getDocumentMetrics`, [fsPath]);
  if (res === null) {
    return undefined;
  }
  return res;
}

async function commandCopyAnsiHighlight(): Promise<void> {
  const editor = activeTypstEditor();
  if (editor === undefined) {
    return;
  }

  const res = await tinymist.exportAnsiHighlight(editor.document.uri.fsPath, {
    range: editor.selection,
  });

  if (res === null) {
    return;
  }

  // copy to clipboard
  await vscode.env.clipboard.writeText(res);
}

async function commandClearCache(): Promise<void> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const uri = activeEditor.document.uri.toString();

  await tinymist.executeCommand("tinymist.doClearCache", [uri]);
}

async function commandPinMain(isPin: boolean): Promise<void> {
  if (!isPin) {
    await tinymist.executeCommand("tinymist.pinMain", [null]);
    return;
  }

  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  await tinymist.executeCommand("tinymist.pinMain", [activeEditor.document.uri.fsPath]);
}

async function initTemplate(context: vscode.ExtensionContext, inPlace: boolean, ...args: string[]) {
  const initArgs: string[] = [];
  if (!inPlace) {
    if (args.length === 2) {
      initArgs.push(...args);
    } else if (args.length > 0) {
      await vscode.window.showErrorMessage(
        "Invalid arguments for initTemplate, needs either all arguments or zero arguments",
      );
      return;
    } else {
      const mode = await getTemplateSpecifier();
      initArgs.push(mode ?? "");
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
    }

    const fsPath = initArgs[1];
    const uri = Uri.file(fsPath);

    interface InitResult {
      entryPath: string;
    }

    const res = await tinymist.executeCommand<InitResult | undefined>(
      "tinymist.doInitTemplate",
      initArgs,
    );

    const workspaceRoot = workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (res && workspaceRoot && uri.fsPath.startsWith(workspaceRoot)) {
      const entry = Uri.file(path.resolve(uri.fsPath, res.entryPath));
      await commands.executeCommand("vscode.open", entry, ViewColumn.Active);
    } else {
      // focus the new folder
      await commands.executeCommand("vscode.openFolder", uri);
    }
  } else {
    if (args.length === 1) {
      initArgs.push(...args);
    } else if (args.length > 0) {
      await vscode.window.showErrorMessage(
        "Invalid arguments for initTemplateInPlace, needs either all arguments or zero arguments",
      );
      return;
    } else {
      const mode = await getTemplateSpecifier();
      initArgs.push(mode ?? "");
    }

    const res = await tinymist.executeCommand<string | undefined>(
      "tinymist.doGetTemplateEntry",
      initArgs,
    );

    if (!res) {
      return;
    }

    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
      return;
    }

    // insert content at the cursor
    activeEditor.edit((editBuilder) => {
      editBuilder.insert(activeEditor.selection.active, res);
    });
  }

  function getTemplateSpecifier(): Promise<string> {
    const data = getUserPackageData(context).data;
    const pkgSpecifiers: string[] = [];
    for (const ns of Object.keys(data)) {
      for (const pkgName of Object.keys(data[ns])) {
        const pkg = data[ns][pkgName];
        if (pkg?.isFavorite) {
          pkgSpecifiers.push(`@${ns}/${pkgName}`);
        }
      }
    }

    return new Promise((resolve) => {
      const quickPick = window.createQuickPick();
      quickPick.placeholder =
        "git, package spec with an optional version, such as `@preview/touying:0.3.2`";
      quickPick.canSelectMany = false;
      quickPick.items = pkgSpecifiers.map((label) => ({ label }));
      quickPick.onDidAccept(() => {
        const selection = quickPick.activeItems[0];
        resolve(selection.label);
        quickPick.hide();
      });
      quickPick.onDidChangeValue(() => {
        // add a new code to the pick list as the first item
        if (!pkgSpecifiers.includes(quickPick.value)) {
          const newItems = [quickPick.value, ...pkgSpecifiers].map((label) => ({
            label,
          }));
          quickPick.items = newItems;
        }
      });
      quickPick.onDidHide(() => quickPick.dispose());
      quickPick.show();
    });
  }
}

async function commandActivateDoc(doc: vscode.TextDocument | undefined): Promise<void> {
  await commandActivateDocPath(doc, doc?.uri.fsPath);
}

async function commandActivateDocPath(
  doc: vscode.TextDocument | undefined,
  fsPath: string | undefined,
): Promise<void> {
  // console.log("focus main", fsPath, new Error().stack);
  extensionState.mut.focusingFile = fsPath;
  if (fsPath) {
    extensionState.mut.focusingDoc = doc;
  }
  if (extensionState.mut.focusingDoc?.isClosed) {
    extensionState.mut.focusingDoc = undefined;
  }
  const formatString = statusBarFormatString();
  // remove the status bar until the last focusing file is closed
  triggerStatusBar(
    !!formatString && !!(fsPath || extensionState.mut.focusingDoc?.isClosed === false),
  );
  await tinymist.executeCommand("tinymist.focusMain", [fsPath]);
}

async function commandRunCodeLens(...args: string[]): Promise<void> {
  if (args.length === 0) {
    return;
  }
  // res.push(doc_lens("Preview in ..", vec!["preview-in".into()]));
  // res.push(doc_lens("Export as ..", vec!["export-as".into()]));

  switch (args[0]) {
    case "profile": {
      void vscode.commands.executeCommand(`tinymist.profileCurrentFile`);
      return;
    }
    case "preview": {
      void vscode.commands.executeCommand(`typst-preview.preview`);
      return;
    }
    case "export-html": {
      await commandShow("Html");
      break;
    }
    case "export-pdf": {
      await commandShow("Pdf");
      return;
    }
    case "more": {
      return codeLensMore();
    }
    default: {
      console.error("unknown code lens command", args[0]);
    }
  }

  async function codeLensMore(): Promise<void> {
    const kBrowsing = "Browsing Preview Documents";
    const kPreviewIn = "Preview in ..";
    const kExportAs = "Export as ..";
    const moreCodeLens = [kBrowsing, kPreviewIn, kExportAs] as const;

    const moreAction = (await vscode.window.showQuickPick(moreCodeLens, {
      title: "More Actions",
    })) as (typeof moreCodeLens)[number] | undefined;

    switch (moreAction) {
      case kBrowsing: {
        void vscode.commands.executeCommand(`tinymist.browsingPreview`);
        return;
      }
      case kPreviewIn: {
        // prompt for enum (doc, slide) with default
        const mode = await vscode.window.showQuickPick(["doc", "slide"], {
          title: "Preview Mode",
        });
        if (mode === undefined) {
          return;
        }
        const target = await vscode.window.showQuickPick(["tab", "browser"], {
          title: "Target to preview in",
        });

        if (target === undefined) {
          return;
        }

        const command =
          (target === "tab" ? "preview" : "browser") + (mode === "slide" ? "-slide" : "");

        void vscode.commands.executeCommand(`typst-preview.${command}`);
        return;
      }
      case kExportAs: {
        enum FastKind {
          PDF = "PDF",
          SVG = "SVG (First Page)",
          SVGMerged = "SVG (Merged)",
          PNG = "PNG (First Page)",
          PNGMerged = "PNG (Merged)",
        }

        const fmt = await vscode.window.showQuickPick(
          [FastKind.PDF, FastKind.SVG, FastKind.SVGMerged, FastKind.PNG, FastKind.PNGMerged],
          {
            title: "Format to export as",
          },
        );

        switch (fmt) {
          case undefined:
            return;
          case FastKind.PDF:
            await commandShow("Pdf");
            return;
          case FastKind.SVG:
            await commandShow("Svg");
            return;
          case FastKind.SVGMerged:
            await commandShow("Svg", { page: { merged: { gap: "0pt" } } });
            return;
          case FastKind.PNG:
            await commandShow("Png");
            return;
          case FastKind.PNGMerged:
            await commandShow("Png", { page: { merged: { gap: "0pt" } } });
            return;
        }

        return;
      }
    }
  }
}

function triggerSuggestAndParameterHints() {
  vscode.commands.executeCommand("editor.action.triggerSuggest");
  vscode.commands.executeCommand("editor.action.triggerParameterHints");
}

export function statusBarFormatString() {
  const formatter = (
    (vscode.workspace.getConfiguration("tinymist").get("statusBarFormat") as string) || ""
  ).trim();

  return formatter;
}
