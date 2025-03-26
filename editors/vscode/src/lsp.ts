import { spawnSync } from "child_process";
import { resolve } from "path";

import * as vscode from "vscode";
import { ExtensionMode } from "vscode";
import type {
  LanguageClient,
  SymbolInformation,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

import { HoverDummyStorage } from "./features/hover-storage";
import type { HoverTmpStorage } from "./features/hover-storage.tmp";
import { extensionState } from "./state";
import { DisposeList, getSensibleTextEditorColumn, typstDocumentSelector } from "./util";
import { substVscodeVarsInConfig, TinymistConfig } from "./config";
import { TinymistStatus, wordCountItemProcess } from "./ui-extends";
import { previewProcessOutline } from "./features/preview";
import { wordPattern } from "./language";

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

/**
 * The result of starting a preview task.
 */
export interface PreviewResult {
  /**
   * The frontend address
   */
  staticServerAddr?: string;
  /**
   * The frontend port
   */
  staticServerPort?: number;
  /**
   * The data plane address
   */
  dataPlanePort?: number;
  /**
   * Whether the preview content is provided by the primary compiler instance. This must be indicate by the CLI argument `--not-primary`
   * when starts a preview task by *LSP Command*.
   *
   * Context: If there is a only preview task, the (primary) compiler instance which is used by LSP is used.
   * If there are multiple preview tasks, tinymist will spawn a new compiler instance for each additional task.
   */
  isPrimary?: boolean;
}

// That's very unfortunate that sourceScrollBySpan doesn't work well.
export interface SourceScrollBySpanRequest {
  event: "sourceScrollBySpan";
  span: string;
}

export interface PanelScrollByPositionRequest {
  event: "panelScrollByPosition";
  position: any;
}

export interface PanelScrollOrCursorMoveRequest {
  event: "panelScrollTo" | "changeCursorPosition";
  filepath: string;
  line: any;
  character: any;
}

export type ScrollPreviewRequest =
  | SourceScrollBySpanRequest
  | PanelScrollByPositionRequest
  | PanelScrollOrCursorMoveRequest;

interface JumpInfo {
  filepath: string;
  start: [number, number] | null;
  end: [number, number] | null;
}

interface ViewportInfo {
  pageNo: number;
  y: number;
}

export class LanguageState {
  static Client: typeof LanguageClient = undefined!;
  static HoverTmpStorage?: typeof HoverTmpStorage = undefined;

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
      ? [[`\`${configName}\` (${configPath})`, configPath]]
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

  initClient(config: TinymistConfig) {
    const context = this.context;
    const isProdMode = context.extensionMode === ExtensionMode.Production;

    /// The `--mirror` flag is only used in development/test mode for testing
    const mirrorFlag = isProdMode ? [] : ["--mirror", "tinymist-lsp.log"];
    /// Set the `RUST_BACKTRACE` environment variable to `full` to print full backtrace on error. This is useless in
    /// production mode because we don't put the debug information in the binary.
    ///
    /// Note: Developers can still download the debug information from the GitHub Releases and enable the backtrace
    /// manually by themselves.
    const RUST_BACKTRACE = isProdMode ? "1" : "full";

    const run = {
      command: config.probedServerPath,
      args: ["lsp", ...mirrorFlag],
      options: { env: Object.assign({}, process.env, { RUST_BACKTRACE }) },
    };
    // console.log("use arguments", run);
    const serverOptions: ServerOptions = {
      run,
      debug: run,
    };

    const trustedCommands = {
      enabledCommands: ["tinymist.openInternal", "tinymist.openExternal"],
    };
    const hoverStorage =
      extensionState.features.renderDocs && LanguageState.HoverTmpStorage
        ? new LanguageState.HoverTmpStorage(context)
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
                /"data:image\/svg\+xml;base64,([^"]*)"/g,
                (_, content: string) => hoverHandler.storeImage(content),
              );
            }
          }

          await hoverHandler.finish();
          return hover;
        },
      },
    };

    const client = (this.client = new LanguageState.Client(
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

    client.onNotification("tinymist/compileStatus", (params: TinymistStatus) => {
      wordCountItemProcess(params);
    });

    this.registerPreviewNotifications(client);

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
   * The commands group for the *Document Preview* feature. This feature is used to preview multiple
   * documents at the same time.
   *
   * A preview task is started by calling {@link startPreview} or {@link startBrowsingPreview} with
   * the *CLI arguments* to pass to the preview task like you would do in the terminal. Although
   * language server will stop a preview task when no connection is active for a while, it can be
   * killed by calling {@link killPreview} with a task id of the preview task.
   *
   * The task id of a preview task is determined by the client. If no task id is provided, you
   * cannot force kill a preview task from client. You also cannot have multiple preview tasks at
   * the same time without specifying it.
   *
   * When a preview task is active, the client can request to scroll preview panel by the calling
   * {@link scrollPreview}. The server will translate client requests and control the preview panel
   * internally.
   *
   * Besides calling commands from the client to the server, a client must also handle notifications
   * from the server. Please check body of {@link registerPreviewNotifications} for a list of them.
   */
  static _GroupDocumentPreviewFeatureCommands = null;

  /**
   * Starts a preview task. See {@link _GroupDocumentPreviewFeatureCommands} for more information.
   *
   * @param previewArgs - The *CLI arguments* to pass to the preview task. See help of the preview
   * CLI command for more information.
   * @returns The result of the preview task.
   */
  async startPreview(previewArgs: string[]): Promise<PreviewResult> {
    const res = await tinymist.executeCommand<PreviewResult>(`tinymist.doStartPreview`, [
      previewArgs,
    ]);
    return res || {};
  }

  /**
   * Starts a browsing preview task. See {@link _GroupDocumentPreviewFeatureCommands} for more information.
   * The difference between this and {@link startPreview} is that the main file will change according to the requests
   * sent to the language server.
   *
   * @param previewArgs - The *CLI arguments* to pass to the preview task. See help of the preview
   * CLI command for more information.
   * @returns The result of the preview task.
   */
  async startBrowsingPreview(previewArgs: string[]): Promise<PreviewResult> {
    const res = await tinymist.executeCommand<PreviewResult>(`tinymist.doStartBrowsingPreview`, [
      previewArgs,
    ]);
    return res || {};
  }

  /**
   * Kills a preview task. See {@link _GroupDocumentPreviewFeatureCommands} for more information.
   *
   * @param taskId - The task ID of the preview task to kill.
   */
  async killPreview(taskId: string): Promise<void> {
    return await tinymist.executeCommand(`tinymist.doKillPreview`, [taskId]);
  }

  /**
   * Scrolls the preview to a specific position. See {@link _GroupDocumentPreviewFeatureCommands}
   * for more information.
   *
   * @param taskId - The task ID of the preview task to scroll.
   * @param req - The request to scroll to.
   */
  async scrollPreview(taskId: string, req: ScrollPreviewRequest): Promise<void> {
    return await tinymist.executeCommand(`tinymist.scrollPreview`, [taskId, req]);
  }

  /**
   * Registers the preview notifications receiving from the language server. See
   * {@link _GroupDocumentPreviewFeatureCommands} for more information.
   */
  registerPreviewNotifications(client: LanguageClient) {
    // (Required) The server requests to dispose (clean up) a preview task when it is no longer
    // needed.
    client.onNotification("tinymist/preview/dispose", ({ taskId }) => {
      const dispose = previewDisposes[taskId];
      if (dispose) {
        dispose();
        delete previewDisposes[taskId];
      } else {
        console.warn("No dispose function found for task", taskId);
      }
    });

    // (Optional) The server requests to scroll the source code to a specific position
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

      function inputHasUri(
        input: unknown,
      ): input is vscode.TabInputText | vscode.TabInputCustom | vscode.TabInputNotebook {
        return (
          input instanceof vscode.TabInputText ||
          input instanceof vscode.TabInputCustom ||
          input instanceof vscode.TabInputNotebook
        );
      }

      // Resolve the affiliated column if it is already opened
      let affiliatedColumn: vscode.ViewColumn | undefined = undefined;
      for (const group of vscode.window.tabGroups.all) {
        for (const tab of group.tabs) {
          if (!tab || !inputHasUri(tab.input)) {
            continue;
          }

          if (tab.input.uri.fsPath === jump.filepath) {
            affiliatedColumn = group.viewColumn;
            break;
          }
        }
        if (affiliatedColumn !== undefined) {
          break;
        }
      }

      // open this file and show in editor
      const doc =
        vscode.workspace.textDocuments.find((doc) => doc.uri.fsPath === jump.filepath) ||
        (await vscode.workspace.openTextDocument(jump.filepath));
      const col = affiliatedColumn || getSensibleTextEditorColumn();
      const editor = await vscode.window.showTextDocument(doc, col);
      const startPosition = new vscode.Position(jump.start[0], jump.start[1]);
      const endPosition = new vscode.Position(jump.end[0], jump.end[1]);
      const range = new vscode.Range(startPosition, endPosition);
      editor.selection = new vscode.Selection(range.start, range.end);
      editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
    });

    // (Optional) The server requests to update the document outline
    client.onNotification("tinymist/documentOutline", async (data: any) => {
      previewProcessOutline(data);
    });

    // (Required) The server requests to report the current viewport of the preview page.
    client.onNotification("tinymist/preview/updateViewport", (viewport: ViewportInfo) => {
      console.log("recv updateViewport request", viewport);
    });
  }

  /**
   * End of {@link _GroupDocumentPreviewFeatureCommands}
   */

  /**
   * The code is borrowed from https://github.com/rust-lang/rust-analyzer/commit/00726cf697271617945b02baa932d2915ebce8b7/editors/code/src/config.ts#L98
   * Last checked time: 2025-03-20
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
          beforeText: /^\s*\/{2}!.*$/,
          action: { indentAction, appendText: "//! " },
        },
        {
          // Begins an auto-closed multi-line comment (standard or parent doc)
          // e.g. /** | */ or /*! | */
          beforeText: /^\s*\/\*(\*|!)(?!\/)([^*]|\*(?!\/))*$/,
          afterText: /^\s*\*\/$/,
          action: {
            indentAction: vscode.IndentAction.IndentOutdent,
            appendText: " * ",
          },
        },
        {
          // Begins a multi-line comment (standard or parent doc)
          // e.g. /** ...| or /*! ...|
          beforeText: /^\s*\/\*(\*|!)(?!\/)([^*]|\*(?!\/))*$/,
          action: { indentAction, appendText: " * " },
        },
        {
          // Continues a multi-line comment
          // e.g.  * ...|
          beforeText: /^( {2})* \*( ([^*]|\*(?!\/))*)?$/,
          action: { indentAction, appendText: "* " },
        },
        {
          // Dedents after closing a multi-line comment
          // e.g.  */|
          beforeText: /^( {2})* \*\/\s*$/,
          action: { indentAction, removeText: 1 },
        },
      ];
    }

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
