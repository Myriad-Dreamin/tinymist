import { spawnSync } from "child_process";
import { resolve } from "path";

import * as vscode from "vscode";
import { ExtensionMode } from "vscode";
import {
  LanguageClient,
  SymbolInformation,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

import { HoverDummyStorage, HoverTmpStorage } from "./features/hover-storage";
import { extensionState } from "./state";
import { DisposeList, getSensibleTextEditorColumn, typstDocumentSelector } from "./util";
import { substVscodeVarsInConfig } from "./config";
import { wordCountItemProcess } from "./ui-extends";
import { previewProcessOutline } from "./features/preview";

interface ResourceRoutes {
  "/fonts": any;
  "/symbols": any;
  "/preview/index.html": string;
  "/dir/package": string;
  "/dir/package/local": string;
  "/package/by-namespace": PackageInfo[];
  "/package/symbol": SymbolInfo;
  "/package/docs": string;
}

/// kill the probe task after 60s
const PROBE_TIMEOUT = 60_000;

class LanguageState {
  outputChannel: vscode.OutputChannel = vscode.window.createOutputChannel("Tinymist Typst", "log");
  context: vscode.ExtensionContext = undefined!;
  client: LanguageClient | undefined = undefined;
  clientPromiseResolve = (_client: LanguageClient) => {};
  clientPromise: Promise<LanguageClient> = new Promise((resolve) => {
    this.clientPromiseResolve = resolve;
  });

  async stop() {
    this.clientPromiseResolve = (_client: LanguageClient) => {};
    this.clientPromise = new Promise((resolve) => {
      this.clientPromiseResolve = resolve;
    });

    if (this.client) {
      await this.client.stop();
      this.client = undefined;
    }
  }

  getClient() {
    return this.clientPromise;
  }

  probeEnvPath(configName: string, configPath?: string): string {
    const isWindows = process.platform === "win32";
    const binarySuffix = isWindows ? ".exe" : "";
    const binaryName = "tinymist" + binarySuffix;

    const serverPaths: [string, string][] = configPath
      ? [[`\`${configName}\` (${configPath})`, configPath as string]]
      : [
          ["Bundled", resolve(__dirname, binaryName)],
          ["In PATH", binaryName],
        ];

    return tinymist.probePaths(serverPaths);
  }

  probePaths(paths: [string, string][]): string {
    const messages = [];
    for (const [loc, path] of paths) {
      let messageSuffix;
      try {
        const result = spawnSync(path, ["probe"], { timeout: PROBE_TIMEOUT });
        if (result.status === 0) {
          return path;
        }

        const statusMessage = result.status !== null ? [`return status: ${result.status}`] : [];
        const errorMessage =
          result.error?.message !== undefined ? [`error: ${result.error.message}`] : [];
        const messages = [statusMessage, errorMessage];
        messageSuffix = messages.length !== 0 ? `:\n\t${messages.flat().join("\n\t")}` : "";
      } catch (e) {
        if (e instanceof Error) {
          messageSuffix = `: ${e.message}`;
        } else {
          messageSuffix = `: ${JSON.stringify(e)}`;
        }
      }

      messages.push([loc, path, `failed to probe${messageSuffix}`]);
    }

    const infos = messages
      .map(([loc, path, message]) => `${loc} ('${path}'): ${message}`)
      .join("\n");
    throw new Error(`Could not find a valid tinymist binary.\n${infos}`);
  }

  initClient(config: Record<string, any>) {
    const context = this.context;
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

    const trustedCommands = {
      enabledCommands: ["tinymist.openInternal", "tinymist.openExternal"],
    };
    const hoverStorage = extensionState.features.renderDocs
      ? new HoverTmpStorage(context)
      : new HoverDummyStorage();

    const clientOptions: LanguageClientOptions = {
      documentSelector: typstDocumentSelector,
      initializationOptions: config,
      outputChannel: this.outputChannel,
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
        provideHover: async (document, position, token, next) => {
          const hover = await next(document, position, token);
          if (!hover) {
            return hover;
          }

          const hoverHandler = await hoverStorage.startHover();

          for (const content of hover.contents) {
            if (content instanceof vscode.MarkdownString) {
              content.isTrusted = trustedCommands;
              content.supportHtml = true;

              if (context.storageUri) {
                content.baseUri = vscode.Uri.joinPath(context.storageUri, "tmp/");
              }

              // outline all data "data:image/svg+xml;base64," to render huge image correctly
              content.value = content.value.replace(
                /\"data\:image\/svg\+xml\;base64,([^\"]*)\"/g,
                (_, content) => hoverHandler.storeImage(content),
              );
            }
          }

          await hoverHandler.finish();
          return hover;
        },
      },
    };

    const client = (this.client = new LanguageClient(
      "tinymist",
      "Tinymist Typst Language Server",
      serverOptions,
      clientOptions,
    ));

    this.clientPromiseResolve(client);
    return client;
  }

  async startClient(): Promise<void> {
    const client = this.client;
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
        vscode.window.activeTextEditor !== undefined,
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

    await client.start();

    return;
  }

  async executeCommand<R>(command: string, args: any[]) {
    return await (
      await this.getClient()
    ).sendRequest<R>("workspace/executeCommand", {
      command,
      arguments: args,
    });
  }

  exportPdf = exportCommand("tinymist.exportPdf");
  exportSvg = exportCommand("tinymist.exportSvg");
  exportPng = exportCommand("tinymist.exportPng");
  exportHtml = exportCommand("tinymist.exportHtml");
  exportMarkdown = exportCommand("tinymist.exportMarkdown");
  exportText = exportCommand("tinymist.exportText");
  exportQuery = exportCommand("tinymist.exportQuery");
  exportAnsiHighlight = exportCommand("tinymist.exportAnsiHighlight");

  getResource<T extends keyof ResourceRoutes>(path: T, ...args: any[]) {
    return tinymist.executeCommand<ResourceRoutes[T]>("tinymist.getResources", [path, ...args]);
  }

  getWorkspaceLabels() {
    return tinymist.executeCommand<SymbolInformation[]>("tinymist.getWorkspaceLabels", []);
  }

  showLog() {
    if (this.client) {
      this.client.outputChannel.show();
    }
  }

  /**
   * The code is borrowed from https://github.com/rust-lang/rust-analyzer/blob/fc98e0657abf3ce07eed513e38274c89bbb2f8ad/editors/code/src/config.ts#L98
   * Last checked time: 2024-11-14
   *
   * Sets up additional language configuration that's impossible to do via a
   * separate language-configuration.json file. See [1] for more information.
   *
   * [1]: https://github.com/Microsoft/vscode/issues/11514#issuecomment-244707076
   */
  configureLang = undefined as vscode.Disposable | undefined;
  configureLanguage(typingContinueCommentsOnNewline: boolean) {
    // Only need to dispose of the config if there's a change
    if (this.configureLang) {
      this.configureLang.dispose();
      this.configureLang = undefined;
    }

    let onEnterRules: vscode.OnEnterRule[] = [
      {
        // Carry indentation from the previous line
        // if it's only whitespace
        beforeText: /^\s+$/,
        action: { indentAction: vscode.IndentAction.None },
      },
      {
        // After the end of a function/field chain,
        // with the semicolon on the same line
        beforeText: /^\s+\..*;/,
        action: { indentAction: vscode.IndentAction.Outdent },
      },
      {
        // After the end of a function/field chain,
        // with semicolon detached from the rest
        beforeText: /^\s+;/,
        previousLineText: /^\s+\..*/,
        action: { indentAction: vscode.IndentAction.Outdent },
      },
    ];

    if (typingContinueCommentsOnNewline) {
      const indentAction = vscode.IndentAction.None;

      onEnterRules = [
        ...onEnterRules,
        {
          // Doc single-line comment
          // e.g. ///|
          beforeText: /^\s*\/{3}.*$/,
          action: { indentAction, appendText: "/// " },
        },
        {
          // Parent doc single-line comment
          // e.g. //!|
          beforeText: /^\s*\/{2}\!.*$/,
          action: { indentAction, appendText: "//! " },
        },
        {
          // Begins an auto-closed multi-line comment (standard or parent doc)
          // e.g. /** | */ or /*! | */
          beforeText: /^\s*\/\*(\*|\!)(?!\/)([^\*]|\*(?!\/))*$/,
          afterText: /^\s*\*\/$/,
          action: {
            indentAction: vscode.IndentAction.IndentOutdent,
            appendText: " * ",
          },
        },
        {
          // Begins a multi-line comment (standard or parent doc)
          // e.g. /** ...| or /*! ...|
          beforeText: /^\s*\/\*(\*|\!)(?!\/)([^\*]|\*(?!\/))*$/,
          action: { indentAction, appendText: " * " },
        },
        {
          // Continues a multi-line comment
          // e.g.  * ...|
          beforeText: /^(\ \ )*\ \*(\ ([^\*]|\*(?!\/))*)?$/,
          action: { indentAction, appendText: "* " },
        },
        {
          // Dedents after closing a multi-line comment
          // e.g.  */|
          beforeText: /^(\ \ )*\ \*\/\s*$/,
          action: { indentAction, removeText: 1 },
        },
      ];
    }

    const wordPattern =
      /(-?\d*.\d\w*)|([^\`\~\!\@\#\%\^\&\*\(\)\=\+\[\{\]\}\\\|\;\:\'\"\,\.<\>\/\?\s]+)/;

    console.log("Setting up language configuration", typingContinueCommentsOnNewline);
    this.configureLang = vscode.languages.setLanguageConfiguration("typst", {
      onEnterRules,
      wordPattern,
    });
  }
}

export const tinymist = new LanguageState();

function exportCommand(command: string) {
  return (uri: string, extraOpts?: any) => {
    return tinymist.executeCommand<string>(command, [uri, ...(extraOpts ? [extraOpts] : [])]);
  };
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

export interface PackageInfo {
  path: string;
  namespace: string;
  name: string;
  version: string;
}

export interface SymbolInfo {
  name: string;
  kind: string;
  children: SymbolInfo[];
}
