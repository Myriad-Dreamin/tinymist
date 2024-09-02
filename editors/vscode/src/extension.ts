import {
  type ExtensionContext,
  workspace,
  window,
  commands,
  ViewColumn,
  Uri,
  TextEditor,
  ExtensionMode,
} from "vscode";
import * as vscode from "vscode";
import * as path from "path";

import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";
import { loadTinymistConfig, substVscodeVarsInConfig } from "./config";
import {
  EditorToolName,
  SymbolViewProvider as SymbolViewProvider,
  activateEditorTool,
  getUserPackageData,
} from "./editor-tools";
import { triggerStatusBar, wordCountItemProcess } from "./ui-extends";
import { setIsTinymist as previewSetIsTinymist } from "./features/preview-compat";
import {
  previewActivate,
  previewDeactivate,
  previewPreload,
  previewProcessOutline,
} from "./features/preview";
import { commandCreateLocalPackage, commandOpenLocalPackage } from "./package-manager";
import { activeTypstEditor, DisposeList, getSensibleTextEditorColumn } from "./util";
import { client, getClient, setClient, tinymist } from "./lsp";
import { taskActivate } from "./features/tasks";
import { onEnterHandler } from "./lsp.on-enter";
import { extensionState } from "./state";
import { devKitFeatureActivate } from "./features/dev-kit";
import { labelFeatureActivate } from "./features/label";

export async function activate(context: ExtensionContext): Promise<void> {
  try {
    return await doActivate(context);
  } catch (e) {
    void window.showErrorMessage(`Failed to activate tinymist: ${e}`);
    throw e;
  }
}

export function deactivate(): Promise<void> | undefined {
  previewDeactivate();
  return client?.stop();
}

export async function doActivate(context: ExtensionContext): Promise<void> {
  const isDevMode = vscode.ExtensionMode.Development == context.extensionMode;
  // Sets a global context key to indicate that the extension is activated
  vscode.commands.executeCommand("setContext", "ext.tinymistActivated", true);
  // Loads configuration
  const config = loadTinymistConfig();
  // Sets features
  extensionState.features.preview = config.previewFeature === "enable";
  extensionState.features.devKit = isDevMode || config.devKit === "enable";
  extensionState.features.onEnter = !!config.onEnterEvent;
  // Initializes language client
  const client = initClient(context, config);
  setClient(client);
  // Activates features
  labelFeatureActivate(context);
  if (extensionState.features.task) {
    taskActivate(context);
  }
  if (extensionState.features.devKit) {
    devKitFeatureActivate(context);
  }
  if (extensionState.features.preview) {
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
    previewActivate(context, false);
  }
  // Starts language client
  return await startClient(client, context);
}

function initClient(context: ExtensionContext, config: Record<string, any>) {
  const isProdMode = context.extensionMode === ExtensionMode.Production;

  const run = {
    command: tinymist.probeEnvPath("tinymist.serverPath", config.serverPath),
    args: [
      "lsp",
      /// The `--mirror` flag is only used in development/test mode for testing
      ...(isProdMode ? [] : ["--mirror", "tinymist-lsp.log"]),
    ],
    options: { env: Object.assign({}, process.env, { RUST_BACKTRACE: "1" }) },
  };
  // console.log("use arguments", run);
  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "typst" },
      { scheme: "untitled", language: "typst" },
    ],
    initializationOptions: config,
    middleware: {
      workspace: {
        async configuration(params, token, next) {
          const items = params.items.map((item) => item.section);
          const result = await next(params, token);
          if (!Array.isArray(result)) {
            return result;
          }
          return substVscodeVarsInConfig(items, result);
        },
      },
    },
  };

  return new LanguageClient(
    "tinymist",
    "Tinymist Typst Language Server",
    serverOptions,
    clientOptions,
  );
}

async function startClient(client: LanguageClient, context: ExtensionContext): Promise<void> {
  if (!client) {
    throw new Error("Language client is not set");
  }

  client.onNotification("tinymist/compileStatus", (params) => {
    wordCountItemProcess(params);
  });

  interface JumpInfo {
    filepath: string;
    start: [number, number] | null;
    end: [number, number] | null;
  }
  client.onNotification("tinymist/preview/scrollSource", async (jump: JumpInfo) => {
    console.log(
      "recv editorScrollTo request",
      jump,
      "active",
      window.activeTextEditor !== undefined,
      "documents",
      vscode.workspace.textDocuments.map((doc) => doc.uri.fsPath),
    );

    if (jump.start === null || jump.end === null) {
      return;
    }

    // open this file and show in editor
    const doc =
      vscode.workspace.textDocuments.find((doc) => doc.uri.fsPath === jump.filepath) ||
      (await vscode.workspace.openTextDocument(jump.filepath));
    const editor = await vscode.window.showTextDocument(doc, getSensibleTextEditorColumn());
    const startPosition = new vscode.Position(jump.start[0], jump.start[1]);
    const endPosition = new vscode.Position(jump.end[0], jump.end[1]);
    const range = new vscode.Range(startPosition, endPosition);
    editor.selection = new vscode.Selection(range.start, range.end);
    editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
  });

  client.onNotification("tinymist/documentOutline", async (data: any) => {
    previewProcessOutline(data);
  });

  client.onNotification("tinymist/preview/dispose", ({ taskId }) => {
    const dispose = previewDisposes[taskId];
    if (dispose) {
      dispose();
      delete previewDisposes[taskId];
    } else {
      console.warn("No dispose function found for task", taskId);
    }
  });

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

  const editorToolCommand = (tool: EditorToolName) => async () => {
    await activateEditorTool(context, tool);
  };

  const initTemplateCommand =
    (inPlace: boolean) =>
    (...args: string[]) =>
      initTemplate(context, inPlace, ...args);

  // prettier-ignore
  context.subscriptions.push(
    commands.registerCommand("tinymist.onEnter", onEnterHandler),

    commands.registerCommand("tinymist.exportCurrentPdf", () => commandExport("Pdf")),
    commands.registerCommand("tinymist.showPdf", () => commandShow("Pdf")),
    commands.registerCommand("tinymist.getCurrentDocumentMetrics", commandGetCurrentDocumentMetrics),
    commands.registerCommand("tinymist.clearCache", commandClearCache),
    commands.registerCommand("tinymist.runCodeLens", commandRunCodeLens),
    commands.registerCommand("tinymist.showLog", tinymist.showLog),
    commands.registerCommand("tinymist.copyAnsiHighlight", commandCopyAnsiHighlight),

    commands.registerCommand("tinymist.pinMainToCurrent", () => commandPinMain(true)),
    commands.registerCommand("tinymist.unpinMain", () => commandPinMain(false)),
    commands.registerCommand("typst-lsp.pinMainToCurrent", () => commandPinMain(true)),
    commands.registerCommand("typst-lsp.unpinMain", () => commandPinMain(false)),

    commands.registerCommand("tinymist.initTemplate", initTemplateCommand(false)),
    commands.registerCommand("tinymist.initTemplateInPlace", initTemplateCommand(true)),

    commands.registerCommand("tinymist.showTemplateGallery", editorToolCommand("template-gallery")),
    commands.registerCommand("tinymist.showSummary", editorToolCommand("summary")),
    commands.registerCommand("tinymist.showSymbolView", editorToolCommand("symbol-view")),
    commands.registerCommand("tinymist.profileCurrentFile", editorToolCommand("tracing")),

    commands.registerCommand("tinymist.createLocalPackage", commandCreateLocalPackage),
    commands.registerCommand("tinymist.openLocalPackage", commandOpenLocalPackage),

    // We would like to define it at the server side, but it is not possible for now.
    // https://github.com/microsoft/language-server-protocol/issues/1117
    commands.registerCommand("tinymist.triggerNamedCompletion", triggerNamedCompletion),
  );
  // context.subscriptions.push
  const provider = new SymbolViewProvider(context);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(SymbolViewProvider.Name, provider),
  );

  await client.start();

  if (extensionState.features.preview) {
    previewPreload(context);
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

  return;
}

async function commandExport(
  mode: "Pdf" | "Svg" | "Png",
  extraOpts?: any,
): Promise<string | undefined> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const uri = activeEditor.document.uri.fsPath;

  const handler = tinymist[`export${mode}`];

  handler(uri, extraOpts);

  const res = await client?.sendRequest<string | null>("workspace/executeCommand", {
    command: `tinymist.export${mode}`,
    arguments: [uri, ...(extraOpts ? [extraOpts] : [])],
  });
  if (res === null) {
    return undefined;
  }
  return res;
}

async function commandGetCurrentDocumentMetrics(): Promise<any> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const fsPath = activeEditor.document.uri.fsPath;

  const res = await client?.sendRequest<string | null>("workspace/executeCommand", {
    command: `tinymist.getDocumentMetrics`,
    arguments: [fsPath],
  });
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

/**
 * Implements the functionality for the 'Show PDF' button shown in the editor title
 * if a `.typ` file is opened.
 */
async function commandShow(kind: "Pdf" | "Svg" | "Png", extraOpts?: any): Promise<void> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  // only create pdf if it does not exist yet
  const exportPath = await commandExport(kind, extraOpts);

  if (exportPath === undefined) {
    // show error message
    await window.showErrorMessage(`Failed to export ${kind}`);
    return;
  }

  const exportUri = Uri.file(exportPath);

  // find and replace exportUri
  // todo: we may find them in tabs
  vscode.window.tabGroups;

  let uriToFind = exportUri.toString();
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
    viewColumn: ViewColumn.Beside,
    preserveFocus: true,
  } as vscode.TextDocumentShowOptions);
}

export interface PreviewResult {
  staticServerPort?: number;
  staticServerAddr?: string;
  dataPlanePort?: number;
  isPrimary?: boolean;
}

const previewDisposes: Record<string, () => void> = {};
export function registerPreviewTaskDispose(taskId: string, dl: DisposeList): void {
  if (previewDisposes[taskId]) {
    throw new Error(`Task ${taskId} already exists`);
  }
  dl.add(() => {
    delete previewDisposes[taskId];
  });
  previewDisposes[taskId] = () => dl.dispose();
}

export async function commandStartPreview(previewArgs: string[]): Promise<PreviewResult> {
  const res = await (
    await getClient()
  ).sendRequest<PreviewResult>("workspace/executeCommand", {
    command: `tinymist.doStartPreview`,
    arguments: [previewArgs],
  });
  return res || {};
}

export async function commandKillPreview(taskId: string): Promise<void> {
  return await (
    await getClient()
  ).sendRequest("workspace/executeCommand", {
    command: `tinymist.doKillPreview`,
    arguments: [taskId],
  });
}

export async function commandScrollPreview(taskId: string, req: any): Promise<void> {
  return await (
    await getClient()
  ).sendRequest("workspace/executeCommand", {
    command: `tinymist.scrollPreview`,
    arguments: [taskId, req],
  });
}

async function commandClearCache(): Promise<void> {
  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  const uri = activeEditor.document.uri.toString();

  await client?.sendRequest("workspace/executeCommand", {
    command: "tinymist.doClearCache",
    arguments: [uri],
  });
}

async function commandPinMain(isPin: boolean): Promise<void> {
  if (!isPin) {
    await client?.sendRequest("workspace/executeCommand", {
      command: "tinymist.pinMain",
      arguments: [null],
    });
    return;
  }

  const activeEditor = window.activeTextEditor;
  if (activeEditor === undefined) {
    return;
  }

  await client?.sendRequest("workspace/executeCommand", {
    command: "tinymist.pinMain",
    arguments: [activeEditor.document.uri.fsPath],
  });
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

    const res: InitResult | undefined = await client?.sendRequest("workspace/executeCommand", {
      command: "tinymist.doInitTemplate",
      arguments: [...initArgs],
    });

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

    const res: string | undefined = await client?.sendRequest("workspace/executeCommand", {
      command: "tinymist.doGetTemplateEntry",
      arguments: [...initArgs],
    });

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
  // remove the status bar until the last focusing file is closed
  triggerStatusBar(!!(fsPath || extensionState.mut.focusingDoc?.isClosed === false));
  await client?.sendRequest("workspace/executeCommand", {
    command: "tinymist.focusMain",
    arguments: [fsPath],
  });
}

async function commandRunCodeLens(...args: string[]): Promise<void> {
  if (args.length === 0) {
    return;
  }

  switch (args[0]) {
    case "profile": {
      void vscode.commands.executeCommand(`tinymist.profileCurrentFile`);
      break;
    }
    case "preview": {
      void vscode.commands.executeCommand(`typst-preview.preview`);
      break;
    }
    case "preview-in": {
      // prompt for enum (doc, slide) with default
      const mode = await vscode.window.showQuickPick(["doc", "slide"], {
        title: "Preview Mode",
      });
      const target = await vscode.window.showQuickPick(["tab", "browser"], {
        title: "Target to preview in",
      });

      const command =
        (target === "tab" ? "preview" : "browser") + (mode === "slide" ? "-slide" : "");

      void vscode.commands.executeCommand(`typst-preview.${command}`);
      break;
    }
    case "export-pdf": {
      await commandShow("Pdf");
      break;
    }
    case "export-as": {
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
        case FastKind.PDF:
          await commandShow("Pdf");
          break;
        case FastKind.SVG:
          await commandShow("Svg");
          break;
        case FastKind.SVGMerged:
          await commandShow("Svg", { page: { merged: { gap: "0pt" } } });
          break;
        case FastKind.PNG:
          await commandShow("Png");
          break;
        case FastKind.PNGMerged:
          await commandShow("Png", { page: { merged: { gap: "0pt" } } });
          break;
      }

      break;
    }
    default: {
      console.error("unknown code lens command", args[0]);
    }
  }
}

function triggerNamedCompletion() {
  vscode.commands.executeCommand("editor.action.triggerSuggest");
  vscode.commands.executeCommand("editor.action.triggerParameterHints");
}
