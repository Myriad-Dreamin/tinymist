import * as vscode from "vscode";
import { commands } from "vscode";
import * as path from "path";
import { readFile, writeFile } from "fs/promises";
import { tinymist } from "../lsp";
import { extensionState, ExtensionContext } from "../state";
import { activeTypstEditor, base64Encode, loadHTMLFile } from "../util";

const USER_PACKAGE_VERSION = "0.0.1";
const FONTS_EXPORT_CONFIGURE_VERSION = "0.0.1";

interface ToolDescriptor {
  command: string;
  title: string;
  description: string;
  toolId: EditorToolName;
}

const toolDesc: Partial<Record<EditorToolName, ToolDescriptor>> = {
  "template-gallery": {
    command: "tinymist.showTemplateGallery",
    title: "Template Gallery",
    description: "Show Template Gallery",
    toolId: "template-gallery",
  },
  summary: {
    command: "tinymist.showSummary",
    title: "Document Summary",
    description: "Show Document Summary",
    toolId: "summary",
  },
  "symbol-view": {
    command: "tinymist.showSymbolView",
    title: "Symbols",
    description: "Show Symbol View",
    toolId: "symbol-view",
  },
  "font-view": {
    command: "tinymist.showFontView",
    title: "Fonts",
    description: "Show Font View",
    toolId: "font-view",
  },
  tracing: {
    command: "tinymist.profileCurrentFile",
    title: "Profiling",
    description: "Profile Current File",
    toolId: "tracing",
  },
};

export function toolFeatureActivate(context: vscode.ExtensionContext) {
  const toolView = new ToolViewProvider();

  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("tinymist.tool-view", toolView),
    ...Object.values(toolDesc).map((desc) =>
      vscode.commands.registerCommand(desc.command, async () => {
        await editorTool(context, desc.toolId);
      }),
    ),
  );
}

export type EditorToolName =
  | "template-gallery"
  | "tracing"
  | "summary"
  | "font-view"
  | "symbol-view"
  | "docs";
export async function editorTool(context: ExtensionContext, tool: EditorToolName, opts?: any) {
  // Create and show a new WebView
  const title = {
    "template-gallery": "Template Gallery",
    "font-view": "Font View",
    "symbol-view": "Symbol View",
    tracing: "Tracing",
    summary: "Summary",
    docs: `@${opts?.pkg?.namespace}/${opts?.pkg?.name}:${opts?.pkg?.version} (Docs)`,
  }[tool];
  const enableFindWidget: Partial<Record<EditorToolName, boolean>> = {
    docs: true,
    "font-view": true,
  };
  const panel = vscode.window.createWebviewPanel(
    `tinymist-${tool}`,
    title,
    {
      viewColumn: vscode.ViewColumn.Beside,
      preserveFocus: tool === "summary" || tool === "tracing",
    }, // Which sides
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      enableFindWidget: !!enableFindWidget[tool],
    },
  );

  await editorToolAt(context, tool, panel, opts);
}

export async function editorToolAt(
  context: ExtensionContext,
  tool: EditorToolName,
  panel: vscode.WebviewView | vscode.WebviewPanel,
  opts?: any,
) {
  const Standalone: Partial<Record<EditorToolName, boolean>> = {
    "symbol-view": true,
  } as const;

  const disposes: vscode.Disposable[] = [];
  const dispose = () => {
    for (const d of disposes) {
      d.dispose();
    }
    // if has dispose method
    if ("dispose" in panel) {
      panel.dispose();
    }
  };

  panel.webview.onDidReceiveMessage(async (message) => {
    console.log("onDidReceiveMessage", message);
    switch (message.type) {
      case "revealPath": {
        const path = message.path;
        const x = await vscode.commands.executeCommand("vscode.open", vscode.Uri.file(path));
        const y = await vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(path));
        console.log("revealPath", x, y);
        break;
      }
      case "savePackageData": {
        const data = message.data;
        context.globalState.update("userPackageData", {
          version: USER_PACKAGE_VERSION,
          data,
        });
        break;
      }
      case "saveFontsExportConfigure": {
        const data = message.data;
        context.globalState.update("fontsExportConfigure", {
          version: FONTS_EXPORT_CONFIGURE_VERSION,
          data,
        });
        break;
      }
      case "initTemplate": {
        const packageSpec = message.packageSpec;
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
        break;
      }
      case "copyToClipboard": {
        vscode.env.clipboard.writeText(message.content);
      }
      case "editText": {
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

        const edit = message.edit;
        if (typeof edit.newText === "string") {
          // replace the selection with the new text
          await editor.edit((editBuilder) => {
            editBuilder.replace(selection, edit.newText);
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
          } = edit.newText;
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
              let range = new vscode.Range(
                selectionStart.with(undefined, selectionStart.character - 1),
                selectionStart,
              );
              const before = selectionStart.character > 0 ? activeDocument.getText(range) : "";
              if (before.match(/[\p{XID_Start}\p{XID_Continue}_]/u)) {
                replaceText = " " + math;
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

        break;
      }
      case "saveDataToFile": {
        let { data, path, option } = message;
        if (typeof path !== "string") {
          const uri = await vscode.window.showSaveDialog(option);
          path = uri?.fsPath;
        }
        if (typeof path !== "string") {
          return;
        }
        await writeFile(path, data);
        break;
      }
      default: {
        console.error("Unknown message type", message.type);
        break;
      }
    }
  });

  let disposed = false;
  panel.onDidDispose(async () => {
    disposed = true;
  });

  const appDir = Standalone[tool] ? tool : "default";
  let html = await loadHTMLFile(context, `./out/editor-tools/${appDir}/index.html`);
  // packageData

  html = html.replace(
    /`editor-tools-args:{"page": [^`]*?`/,
    `\`editor-tools-args:{"page": "${tool}"}\``,
  );

  let afterReloadHtml = undefined;

  switch (tool) {
    case "template-gallery":
      const userPackageData = getUserPackageData(context);
      const packageData = JSON.stringify(userPackageData.data);
      html = html.replace(":[[preview:FavoritePlaceholder]]:", base64Encode(packageData));
      break;
    case "tracing": {
      const focusingFile = extensionState.getFocusingFile();
      if (focusingFile === undefined) {
        await vscode.window.showErrorMessage("No focusing typst file");
        return;
      }
      const traceDataTask = vscode.commands.executeCommand(
        "tinymist.getDocumentTrace",
        focusingFile,
      );

      // do that after the html is reloaded
      afterReloadHtml = async () => {
        const traceData = await traceDataTask;
        if (!disposed) {
          panel.webview.postMessage({ type: "traceData", data: traceData });
        }
      };

      break;
    }
    case "summary": {
      const fontsExportConfigure = getFontsExportConfigure(context);
      const fontsExportConfig = JSON.stringify(fontsExportConfigure.data);
      const [docMetrics, serverInfo] = await fetchSummaryInfo();

      if (!docMetrics || !serverInfo) {
        if (!docMetrics) {
          vscode.window.showErrorMessage("No document metrics available");
        }
        if (!serverInfo) {
          vscode.window.showErrorMessage("No server info");
        }

        dispose();
        return;
      }

      html = html.replace(":[[preview:FontsExportConfigure]]:", base64Encode(fontsExportConfig));
      html = html.replace(":[[preview:DocumentMetrics]]:", base64Encode(docMetrics));
      html = html.replace(":[[preview:ServerInfo]]:", base64Encode(serverInfo));
      break;
    }
    case "font-view": {
      const result = await tinymist.getResource("/fonts");

      if (!result) {
        vscode.window.showErrorMessage("No resource");
        dispose();
        return;
      }

      const fontInfo = JSON.stringify(result);
      html = html.replace(":[[preview:FontInformation]]:", base64Encode(fontInfo));

      let version = 0;

      const processSelections = async (
        selectionVersion: number,
        textEditor: vscode.TextEditor | undefined,
        selections: readonly vscode.Selection[],
      ) => {
        console.log(selections);
        // todo: very buggy so we disabling it
        if (!textEditor || selections.length >= 0) {
          return undefined;
        }

        if (!textEditor || selections.length > 1) {
          return;
        }

        for (const sel of selections) {
          console.log(textEditor, sel.start);
          const textDocument = {
            uri: textEditor.document.uri.toString(),
          };
          const position = {
            line: sel.start.line,
            character: sel.start.character,
          };
          const style = ["text.font"];
          const styleAt = (
            await vscode.commands.executeCommand<{ style: any[] }[]>(
              "tinymist.interactCodeContext",
              {
                textDocument,
                query: [
                  {
                    kind: "styleAt",
                    position,
                    style,
                  },
                ],
              },
            )
          )?.[0]?.style;

          return {
            version: selectionVersion,
            selections: [
              {
                textDocument,
                position,
                style,
                styleAt,
              },
            ],
          };
        }
      };

      disposes.push(
        vscode.window.onDidChangeTextEditorSelection(async (event) => {
          if (disposed) {
            return;
          }
          if (!event.textEditor || event.textEditor.document.languageId !== "typst") {
            return;
          }
          version += 1;
          const styleAtCursor = await processSelections(
            version,
            event.textEditor,
            event.selections,
          );
          if (disposed) {
            return;
          }
          panel.webview.postMessage({ type: "styleAtCursor", data: styleAtCursor });
        }),
      );

      const activeEditor = activeTypstEditor();
      if (activeEditor) {
        const styleAtCursor = await processSelections(
          version,
          activeEditor,
          activeEditor.selections,
        );
        if (styleAtCursor) {
          html = html.replace(
            ":[[preview:StyleAtCursor]]:",
            base64Encode(JSON.stringify(styleAtCursor)),
          );
        }
      }

      break;
    }
    case "symbol-view": {
      const result = await tinymist.getResource("/symbols");

      if (!result) {
        vscode.window.showErrorMessage("No resource");
        dispose();
        return;
      }

      const symbolInfo = JSON.stringify(result);
      html = html.replace(":[[preview:SymbolInformation]]:", base64Encode(symbolInfo));
      break;
    }
    case "docs": {
      html = html.replace(":[[preview:DocContent]]:", base64Encode(opts.content));
      break;
    }
  }

  panel.webview.html = html;

  if (afterReloadHtml) {
    afterReloadHtml();
  }
}

export function getUserPackageData(context: ExtensionContext) {
  const defaultPackageData: Versioned<PackageData> = {
    version: USER_PACKAGE_VERSION,
    data: {},
  };

  const userPackageData = context.globalState.get("userPackageData", defaultPackageData);
  if (userPackageData?.version !== USER_PACKAGE_VERSION) {
    return defaultPackageData;
  }

  return userPackageData;
}

// todo: deduplicate me. it also occurs in tools/editor-tools/src/features/summary.ts
export const fontsExportDefaultConfigure: fontsExportConfigure = {
  format: "csv",
  filters: {
    location: ["fs"],
  },
  csv: {
    header: false,
    delimiter: ",",
    fields: ["name", "path"],
  },
  json: {
    indent: 2,
  },
};

export function getFontsExportConfigure(context: ExtensionContext) {
  const defaultConfigure: Versioned<fontsExportConfigure> = {
    version: FONTS_EXPORT_CONFIGURE_VERSION,
    data: fontsExportDefaultConfigure,
  };

  const configure = context.globalState.get("fontsExportConfigure", defaultConfigure);
  if (configure?.version !== FONTS_EXPORT_CONFIGURE_VERSION) {
    return defaultConfigure;
  }

  return configure;
}

interface Versioned<T> {
  version: string;
  data: T;
}

export interface PackageData {
  [ns: string]: {
    [packageName: string]: {
      isFavorite: boolean;
    };
  };
}

interface FsFontSource {
  kind: "fs";
  path: string;
}

interface MemoryFontSource {
  kind: "memory";
  name: string;
}

type FontSource = FsFontSource | MemoryFontSource;

export type fontLocation = FontSource extends { kind: infer Kind } ? Kind : never;

export type fontsCSVHeader =
  | "name"
  | "postscript"
  | "style"
  | "weight"
  | "stretch"
  | "location"
  | "path";

export interface fontsExportCSVConfigure {
  header: boolean;
  delimiter: string;
  fields: fontsCSVHeader[];
}

export interface fontsExportJSONConfigure {
  indent: number;
}

export interface fontsExportFormatConfigure {
  csv: fontsExportCSVConfigure;
  json: fontsExportJSONConfigure;
}

export type fontsExportFormat = keyof fontsExportFormatConfigure;

interface fontsExportCommonConfigure {
  format: fontsExportFormat;
  filters: {
    location: fontLocation[];
  };
}

export type fontsExportConfigure = fontsExportCommonConfigure & fontsExportFormatConfigure;

const waitTimeList = [100, 200, 400, 1000, 1200, 1500, 1800, 2000];
async function fetchSummaryInfo(): Promise<[any | undefined, any | undefined]> {
  let res: [any | undefined, any | undefined] = [undefined, undefined];

  for (const to of waitTimeList) {
    const focusingFile = extensionState.getFocusingFile();
    if (focusingFile === undefined) {
      await vscode.window.showErrorMessage("No focusing typst file");
      return res;
    }

    await work(focusingFile, res);
    if (res[0] && res[1]) {
      break;
    }
    // wait for a bit
    await new Promise((resolve) => setTimeout(resolve, to));
  }

  return res;

  async function work(focusingFile: string, res: [any | undefined, any | undefined]) {
    if (!res[0]) {
      const result = await vscode.commands.executeCommand(
        "tinymist.getDocumentMetrics",
        focusingFile,
      );
      if (!result) {
        return;
      }
      const docMetrics = JSON.stringify(result);
      res[0] = docMetrics;
    }

    if (!res[1]) {
      const result2 = await vscode.commands.executeCommand("tinymist.getServerInfo");
      if (!result2) {
        return;
      }
      const serverInfo = JSON.stringify(result2);
      res[1] = serverInfo;
    }
  }
}

class ToolViewProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
  constructor() {}

  refresh(): void {}

  getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(): Thenable<vscode.TreeItem[]> {
    return Promise.resolve([
      ...Object.values(toolDesc).map((desc) => {
        return new CommandItem({
          title: desc.title,
          command: desc.command,
          tooltip: desc.description,
        });
      }),
    ]);
  }
}

class CommandItem extends vscode.TreeItem {
  constructor(
    public readonly command: vscode.Command,
    public description = "",
  ) {
    super(command.title, vscode.TreeItemCollapsibleState.None);
    this.tooltip = this.command.tooltip || ``;
  }

  iconPath = new vscode.ThemeIcon("tools");

  contextValue = "tool-command";
}
