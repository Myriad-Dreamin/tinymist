import { spawnSync } from "child_process";
import { resolve } from "path";

import * as vscode from "vscode";
import * as lc from "vscode-languageclient";
import * as Is from "vscode-languageclient/lib/common/utils/is";
import type { SymbolInformation, LanguageClientOptions } from "vscode-languageclient/node";
import type { BaseLanguageClient as LanguageClient } from "vscode-languageclient";

import { HoverDummyStorage } from "./features/hover-storage";
import type { HoverTmpStorage } from "./features/hover-storage.tmp";
import { extensionState } from "./state";
import {
  bytesBase64Encode,
  DisposeList,
  getSensibleTextEditorColumn,
  typstDocumentSelector,
} from "./util";
import type { ExportActionOpts, ExportOpts } from "./cmd.export";
import { substVscodeVarsInConfig, TinymistConfig } from "./config";
import { TinymistStatus, wordCountItemProcess } from "./ui-extends";
import { previewProcessOutline } from "./features/preview";
import { wordPattern } from "./language";
import type { createSystemLanguageClient } from "./lsp.system";

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

export class LanguageState {
  static Client: typeof createSystemLanguageClient = undefined!;
  static HoverTmpStorage?: typeof HoverTmpStorage = undefined;

  outputChannel: vscode.OutputChannel = vscode.window.createOutputChannel("Tinymist Typst", "log");
  context: vscode.ExtensionContext = undefined!;
  client: LanguageClient | undefined = undefined;
  _watcher: vscode.FileSystemWatcher | undefined = undefined;
  delegateFsRequests = false;
  // disposables for the watch fallbacks
  private _watchDisposables: vscode.Disposable[] = [];
  clientPromiseResolve = (_client: LanguageClient) => { };
  clientPromise: Promise<LanguageClient> = new Promise((resolve) => {
    this.clientPromiseResolve = resolve;
  });

  async stop() {
    this.clientPromiseResolve = (_client: LanguageClient) => { };
    this.clientPromise = new Promise((resolve) => {
      this.clientPromiseResolve = resolve;
    });

    if (this._watcher) {
      this._watcher.dispose();
      this._watcher = undefined;
    }
    for (const d of this._watchDisposables) {
      try {
        d.dispose();
      } catch (e) {
        console.error("failed to dispose watch disposable", e);
      }
    }
    this._watchDisposables = [];
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

  async initClient(config: TinymistConfig) {
    const context = this.context;

    const trustedCommands = {
      enabledCommands: ["tinymist.openInternal", "tinymist.openExternal", "tinymist.replaceText"],
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

              // https://github.com/James-Yu/LaTeX-Workshop/blob/a0267e507867ae8be94b48a70d0541865fcf905f/src/preview/hover/ongraphics.ts

              // outline all data "data:image/svg+xml;base64," to render huge image correctly
              // Workaround for https://github.com/microsoft/vscode/issues/137632
              // https://github.com/microsoft/vscode/issues/97759
              if (vscode.env.remoteName) {
              } else {
                if (context.storageUri) {
                  content.baseUri = vscode.Uri.joinPath(context.storageUri, "tmp/");
                }

                content.value = content.value.replace(
                  /"data:image\/svg\+xml;base64,([^"]*)"/g,
                  (_, content: string) => `"${hoverHandler.storeImage(content)}"`,
                );
              }
            }
          }

          await hoverHandler.finish();
          return hover;
        },
        // Using custom handling of CodeActions to support action groups and snippet edits.
        // Note that this means we have to re-implement lazy edit resolving ourselves as well.
        async provideCodeActions(
          document: vscode.TextDocument,
          range: vscode.Range,
          context: vscode.CodeActionContext,
          token: vscode.CancellationToken,
          _next: lc.ProvideCodeActionsSignature,
        ) {
          const params: lc.CodeActionParams = {
            textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(document),
            range: client.code2ProtocolConverter.asRange(range),
            context: await client.code2ProtocolConverter.asCodeActionContext(context, token),
          };
          const callback = async (
            values: (lc.Command | lc.CodeAction)[] | null,
          ): Promise<(vscode.Command | vscode.CodeAction)[] | undefined> => {
            if (values === null) return undefined;
            const result: (vscode.CodeAction | vscode.Command)[] = [];
            for (const item of values) {
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              const kind = client.protocol2CodeConverter.asCodeActionKind((item as any).kind);
              const action = new vscode.CodeAction(item.title, kind);
              action.command = {
                command: "tinymist.resolveCodeAction",
                title: item.title,
                arguments: [item],
              };
              // console.log("replace", action, "=>", action);

              // Set a dummy edit, so that VS Code doesn't try to resolve this.
              action.edit = new vscode.WorkspaceEdit();
              result.push(action);
            }
            return result;
          };
          return client
            .sendRequest(lc.CodeActionRequest.type, params, token)
            .then(callback, (_error) => undefined);
        },
      },
    };
    this.delegateFsRequests = !!(config as any).delegateFsRequests;
    const client = (this.client = await LanguageState.Client(context, config, clientOptions));

    this.clientPromiseResolve(client);
    return client;
  }

  async startClient(): Promise<void> {
    const client = this.client;
    console.log("this.client", !!this.client);
    if (!client) {
      throw new Error("Language client is not set");
    }

    this.registerClientSideWatch(client);
    client.onNotification("tinymist/compileStatus", (params: TinymistStatus) => {
      wordCountItemProcess(params);
    });
    if (extensionState.features.preview) {
      this.registerPreviewNotifications(client);
    }

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
  exportTeX = exportCommand("tinymist.exportTeX");
  exportText = exportCommand("tinymist.exportText");
  exportQuery = exportCommand("tinymist.exportQuery");
  exportAnsiHighlight = exportStringCommand("tinymist.exportAnsiHighlight");
  exportAst = exportStringCommand("tinymist.exportAst");

  getResource<T extends keyof ResourceRoutes>(path: T, ...args: any[]) {
    return tinymist.executeCommand<ResourceRoutes[T]>("tinymist.getResources", [path, ...args]);
  }

  getWorkspaceLabels() {
    return tinymist.executeCommand<SymbolInformation[]>("tinymist.getWorkspaceLabels", []);
  }

  interactCodeContext<Qs extends InteractCodeContextQuery[]>(
    documentUri: string | vscode.Uri,
    query: Qs,
  ): Promise<InteractCodeContextResponses<Qs> | undefined> {
    return tinymist.executeCommand("tinymist.interactCodeContext", [
      {
        textDocument: {
          uri: typeof documentUri !== "string" ? documentUri.toString() : documentUri,
        },
        query,
      },
    ]);
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
   * Kills all preview tasks. See {@link _GroupDocumentPreviewFeatureCommands} for more information.
   */
  async killAllPreview(): Promise<void> {
    return await tinymist.executeCommand(`tinymist.doKillPreview`, []);
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
   * Scrolls all the preview to some position. See {@link _GroupDocumentPreviewFeatureCommands}
   * for more information.
   */
  async scrollAllPreview(): Promise<void> {
    return await tinymist.executeCommand(`tinymist.scrollPreview`, []);
  }

  registerClientSideWatch(client: LanguageClient) {
    // clear any existing listeners from a previous client instances
    for (const d of this._watchDisposables) {
      try {
        d.dispose();
      } catch (e) {
        console.error("failed to dispose watch disposable", e);
      }
    }
    this._watchDisposables = [];

    const watches = new Set<string>();
    const hasRead = new Map<string, [number, FileResult | undefined]>();
    let watchClock = 0;

    const tryRead = async (uri: vscode.Uri) => {
      {
        // Virtual workspaces don't provide a filesystem provider that supports workspace.fs.readFile
        // Uses the open TextDocument whenever available.
        const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri.toString());
        if (doc) {
          const text = doc.getText();
          const data = Buffer.from(text, "utf8");
          return { type: "ok", content: bytesBase64Encode(data) } as const;
        }
      }

      // Otherwise falls back to workspace.fs for real files
      return vscode.workspace.fs.readFile(uri).then(
        (data): FileResult => {
          return { type: "ok", content: bytesBase64Encode(data) } as const;
        },
        (err: any): FileResult => {
          console.error("Failed to read file", uri, err);
          return { type: "err", error: err.message as string } as const;
        },
      );
    };

    const registerHasRead = (uri: string, currentClock: number, content?: FileResult) => {
      const previous = hasRead.get(uri);
      if (previous && previous[0] >= currentClock) {
        return false;
      }
      hasRead.set(uri, [currentClock, content]);
      return true;
    };

    let watcher = () => {
      if (this._watcher) {
        return this._watcher;
      }
      console.log("registering watcher");

      this._watcher = vscode.workspace.createFileSystemWatcher("**/*");

      const watchRead = async (currentClock: number, uri: vscode.Uri) => {
        console.log("watchRead", uri, currentClock, watches);
        const uriStr = uri.toString();
        if (!watches.has(uriStr)) {
          return;
        }

        const content = await tryRead(uri);
        if (!registerHasRead(uriStr, currentClock, content)) {
          return;
        }

        const inserts: FileChange[] = [{ uri: uriStr, content }];
        const removes: string[] = [];

        client.sendRequest(fsChange, { inserts, removes, isSync: false });
      };

      this._watcher.onDidChange((uri) => {
        const currentClock = watchClock++;
        console.log("fs change", uri, currentClock);
        watchRead(currentClock, uri);
      });
      this._watcher.onDidCreate((uri) => {
        const currentClock = watchClock++;
        console.log("fs create", uri, currentClock);
        watchRead(currentClock, uri);
      });
      this._watcher.onDidDelete((uri) => {
        const currentClock = watchClock++;
        console.log("fs delete", uri, currentClock);
        watchRead(currentClock, uri);
      });

      return this._watcher;
    };

    // todo: move registering to initClient to avoid unhandled errors.
    client.onRequest("tinymist/fs/watch", (params: FsWatchRequest) => {
      const currentClock = watchClock++;
      console.log(
        "fs watch request",
        params,
        vscode.workspace.workspaceFolders?.map((folder) => folder.uri.toString()),
      );

      const filesToRead: vscode.Uri[] = [];
      const filesDeleted: string[] = [];

      for (const uriStr of params.inserts) {
        const uriObj = vscode.Uri.parse(uriStr);
        if (!watches.has(uriStr)) {
          filesToRead.push(uriObj);
          watches.add(uriStr);
        }
      }

      for (const uriStr of params.removes) {
        if (watches.has(uriStr)) {
          filesDeleted.push(uriStr);
          watches.delete(uriStr);
        }
      }

      const removes: string[] = filesDeleted.filter((path) => {
        return registerHasRead(path, currentClock, undefined);
      });

      (async () => {
        watcher();

        const readFiles = await Promise.all(filesToRead.map((uri) => tryRead(uri)));

        const inserts: FileChange[] = filesToRead
          .map((uri, idx) => ({
            uri: uri.toString(),
            content: readFiles[idx],
          }))
          .filter((change) => registerHasRead(change.uri, currentClock, change.content));

        console.log("fs watch read", currentClock, inserts, removes);
        client.sendRequest(fsChange, { inserts, removes, isSync: true });
      })();
    });

    // in delegated filesystem mode events dont surface through createFileSystemWatcher
    // keep the preview's delegated view in sync by using text document notifications for any URI that the server asked us to watch
    if (this.delegateFsRequests) {
      const sendForDocument = async (doc: vscode.TextDocument, isSync: boolean) => {
        const uriStr = doc.uri.toString();
        const currentClock = watchClock++;
        if (!watches.has(uriStr)) {
          return;
        }

        // we already have the latest text in memory, so send that directly instead of going through workspace.fs.readFile
        const text = doc.getText();
        const data = Buffer.from(text, "utf8");
        const content: FileResult = { type: "ok", content: bytesBase64Encode(data) };
        if (!registerHasRead(uriStr, currentClock, content)) {
          return;
        }

        const inserts: FileChange[] = [{ uri: uriStr, content }];
        const removes: string[] = [];

        client.sendRequest(fsChange, { inserts, removes, isSync }).then(
          () => {
            console.log("sent fsChange (doc)", uriStr, currentClock, { isSync });
          },
          (err) => {
            console.error("fsChange request failed (doc)", uriStr, err);
          },
        );
      };
    }
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

// Type definitions for export responses (matches Rust OnExportResponse)
export type ExportResponse =
  | { path: string | null; data: string | null } // Single
  | { totalPages: number; items: ExportedPage[] }; // Multiple

type ExportedPage = { page: number; path: string | null; data: string | null };

function exportCommand(command: string) {
  return (
    uri: string,
    extraOpts?: ExportOpts,
    actions?: ExportActionOpts,
  ): Promise<ExportResponse | null> => {
    return tinymist.executeCommand<ExportResponse | null>(command, [
      uri,
      extraOpts ?? {},
      actions ?? {},
    ]);
  };
}

function exportStringCommand(command: string) {
  return (uri: string, extraOpts?: ExportOpts): Promise<string> => {
    return tinymist.executeCommand<string>(command, [uri, extraOpts ?? {}]);
  };
}

type InteractCodeContextQuery = PathAtQuery | ModeAtQuery | StyleAtQuery;
type LspPosition = {
  line: number;
  character: number;
};
interface PathAtQuery {
  kind: "pathAt";
  code: string;
  inputs?: Record<string, string>;
}
interface ModeAtQuery {
  kind: "modeAt";
  position: LspPosition;
}
interface StyleAtQuery {
  kind: "styleAt";
  position: LspPosition;
  style: string[];
}
type InteractCodeContextResponses<Qs extends [...InteractCodeContextQuery[]]> = {
  [Index in keyof Qs]: InteractCodeContextResponse<Qs[Index]>;
} & { length: Qs["length"] };
type InteractCodeContextResponse<Q extends InteractCodeContextQuery> = Q extends PathAtQuery
  ? CodeContextQueryResult
  : Q extends ModeAtQuery
  ? ModeAtQueryResult
  : Q extends StyleAtQuery
  ? StyleAtQueryResult
  : never;
export type CodeContextQueryResult<T = any> =
  | {
    value: T;
  }
  | {
    error: string;
  };
export type InterpretMode = "math" | "markup" | "code" | "comment" | "string" | "raw";
export type StyleAtQueryResult = {
  style: any[];
};
export type ModeAtQueryResult = {
  mode: InterpretMode;
};

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

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function isCodeActionWithoutEditsAndCommands(value: any): boolean {
  const candidate: lc.CodeAction = value;
  return (
    candidate &&
    Is.string(candidate.title) &&
    (candidate.diagnostics === void 0 || Is.typedArray(candidate.diagnostics, lc.Diagnostic.is)) &&
    (candidate.kind === void 0 || Is.string(candidate.kind)) &&
    candidate.edit === void 0 &&
    candidate.command === void 0
  );
}

interface FsWatchRequest {
  // delivered by vscode-languageclient
  // can be strings or vscode.Uri objects depending on the transport.
  inserts: string[];
  removes: string[];
}

interface FileResult {
  type: "ok" | "err";
  content?: string;
  error?: string;
}

interface FileChange {
  uri: string;
  content: FileResult;
}

/**
 * A parameter literal used in requests to pass a list of file changes.
 */
export interface FsChangeParams {
  inserts: FileChange[];
  removes: string[];
  isSync: boolean;
}

const fsChange = new lc.RequestType<FsChangeParams, void, void>("tinymist/fsChange");
