// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import * as path from "path";
import { ChildProcessWithoutNullStreams } from "child_process";
import { spawn } from "cross-spawn";
import { WebSocket } from "ws";
import {
  ScrollSyncMode,
  ScrollSyncModeEnum,
  contentPreviewProvider,
  openPreviewInWebView,
  previewProcessOutline,
} from "./preview";
import { tinymist } from "../lsp";
import { loadHTMLFile } from "../util";

import { vscodeVariables } from "../vscode-variables";

let isTinymist = false;
let guy = "$(typst-guy)";

interface TaskControlBlock {
  /// related panel
  panel?: vscode.WebviewPanel;
  /// channel to communicate with typst-preview
  addonΠserver: Addon2Server;
  /// static file server port
  staticFilePort?: string;
}
const activeTask = new Map<vscode.TextDocument, TaskControlBlock>();

export async function setIsTinymist() {
  isTinymist = true;
  guy = "$(sync)";
}

function getCliPath(): string {
  const cfgName = "typst-preview.executable";
  return tinymist.probeEnvPath(cfgName, vscode.workspace.getConfiguration().get<string>(cfgName));
}

let outputChannel: vscode.OutputChannel | undefined = undefined;
export function previewActiveCompat(context: vscode.ExtensionContext) {
  // Use the console to output diagnostic information (console.log) and errors (console.error)
  // This line of code will only be executed once when your extension is activated
  // The command has been defined in the package.json file
  // Now provide the implementation of the command with registerCommand
  // The commandId parameter must match the command field in package.json
  outputChannel = vscode.window.createOutputChannel("typst-preview");

  context.subscriptions.push(
    statusBarInit(),
    vscode.commands.registerCommand("typst-preview.showLog", async () => {
      outputChannel?.show();
    }),
  );
  process.on("SIGINT", () => {
    for (const serverProcess of serverProcesses) {
      serverProcess.kill();
    }
  });

  context.subscriptions.push(
    vscode.commands.registerCommand("typst-preview.showAwaitTree", async () => {
      if (activeTask.size === 0) {
        vscode.window.showWarningMessage("No active preview");
        return;
      }
      vscode.window.showInformationMessage("await tree feature is deprecated...");
    }),
  );
}

// This method is called when your extension is deactivated
export function previewDeactivate() {
  console.log("killing preview services, count", activeTask.size);
  for (const [_, task] of activeTask) {
    task.panel?.dispose();
  }
  for (const serverProcess of serverProcesses) {
    serverProcess.kill();
  }
}

let _previewHtml: string | undefined = undefined;
/**
 * Get the preview html content
 * @param context The extension context
 * @returns The preview html content containing placeholders to fill.
 */
export async function getPreviewHtml(context: vscode.ExtensionContext) {
  if (_previewHtml) {
    return _previewHtml;
  }

  let html;
  if (isTinymist) {
    // Gets the HTML resource from the server
    html = await tinymist.getResource("/preview/index.html");
  } else {
    // In old typst-preview extension, the resources are packed in the extension.
    html = await loadHTMLFile(context, "./out/frontend/index.html");
  }

  if (typeof html === "string") {
    _previewHtml = html;
  }

  // Dispose the `_previewHtml` when the extension is deactivated.
  //
  // This is crucial because people may activate another version of the
  // extension which contains another version of the `_previewHtml`.
  //
  // Not disposing the `_previewHtml` will cause inconsistency between the
  // HTML and language server.
  context.subscriptions.push({
    dispose: () => {
      _previewHtml = undefined;
    },
  });

  return html;
}

let statusBarItem: vscode.StatusBarItem;

export function statusBarInit() {
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  statusBarItem.name = "typst-preview";
  statusBarItem.command = "typst-preview.showLog";
  statusBarItem.tooltip = "Typst Preview Status: Click to show logs";
  return statusBarItem;
}

function statusBarItemProcess(event: "Compiling" | "CompileSuccess" | "CompileError") {
  if (isTinymist) {
    return;
  }

  const style =
    vscode.workspace.getConfiguration().get<string>("typst-preview.statusBarIndicator") ||
    "compact";
  if (statusBarItem) {
    if (event === "Compiling") {
      if (style === "compact") {
        statusBarItem.text = "$(sync~spin)";
      } else if (style === "full") {
        statusBarItem.text = "$(sync~spin) Compiling";
      }
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.prominentBackground");
      statusBarItem.show();
    } else if (event === "CompileSuccess") {
      if (style === "compact") {
        statusBarItem.text = `${guy}`;
      } else if (style === "full") {
        statusBarItem.text = `${guy} Compile Success`;
      }
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.prominentBackground");
      statusBarItem.show();
    } else if (event === "CompileError") {
      if (style === "compact") {
        statusBarItem.text = `${guy}`;
      } else if (style === "full") {
        statusBarItem.text = `${guy} Compile Error`;
      }
      statusBarItem.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
      statusBarItem.show();
    }
  }
}

const serverProcesses: Array<any> = [];

interface LaunchCliResult {
  serverProcess: ChildProcessWithoutNullStreams;
  controlPlanePort: string;
  dataPlanePort: string;
  staticFilePort: string;
}

function runServer(
  command: string,
  projectRoot: string,
  args: string[],
  outputChannel: vscode.OutputChannel,
): Promise<LaunchCliResult> {
  const serverProcess = spawn(command, args, {
    env: {
      ...process.env,
      RUST_BACKTRACE: "1",
    },
    cwd: projectRoot,
  });
  serverProcess.on("error", (err: any) => {
    console.error("Failed to start server process");
    vscode.window.showErrorMessage(`Failed to start typst-preview(${command}) process: ${err}`);
  });
  serverProcess.stdout.on("data", (data: Buffer) => {
    outputChannel.append(data.toString());
  });
  serverProcess.stderr.on("data", (data: Buffer) => {
    outputChannel.append(data.toString());
  });
  serverProcess.on("exit", async (code: any) => {
    if (code !== null && code !== 0) {
      const response = await vscode.window.showErrorMessage(
        `typst-preview process exited with code ${code}`,
        "Show Logs",
      );
      if (response === "Show Logs") {
        outputChannel.show();
      }
    }
    console.log(`child process exited with code ${code}`);
  });

  serverProcesses.push(serverProcesses);
  return new Promise((resolve, _reject) => {
    let dataPlanePort: string | undefined = undefined;
    let controlPlanePort: string | undefined = undefined;
    let staticFilePort: string | undefined = undefined;
    serverProcess.stderr.on("data", (data: Buffer) => {
      if (data.toString().includes("listening on")) {
        console.log(data.toString());
        const ctrlPort = data
          .toString()
          .match(/Control plane server listening on: 127\.0\.0\.1:(\d+)/)?.[1];
        const dataPort = data
          .toString()
          .match(/Data plane server listening on: 127\.0\.0\.1:(\d+)/)?.[1];
        const staticPort = data
          .toString()
          .match(/Static file server listening on: 127\.0\.0\.1:(\d+)/)?.[1];
        if (ctrlPort !== undefined) {
          controlPlanePort = ctrlPort;
        }
        if (dataPort !== undefined) {
          dataPlanePort = dataPort;
        }
        if (staticPort !== undefined) {
          staticFilePort = staticPort;
        }
        if (
          dataPlanePort !== undefined &&
          controlPlanePort !== undefined &&
          staticFilePort !== undefined
        ) {
          resolve({ dataPlanePort, controlPlanePort, staticFilePort, serverProcess });
        }
      }
    });
  });
}

interface LaunchTask {
  context: vscode.ExtensionContext;
  editor: vscode.TextEditor;
  bindDocument: vscode.TextDocument;
  mode: "doc" | "slide";
  webviewPanel?: vscode.WebviewPanel;
  isBrowsing?: boolean;
  isDev?: boolean;
  isNotPrimary?: boolean;
}

export interface LaunchInBrowserTask extends LaunchTask {
  kind: "browser";
}

export interface LaunchInWebViewTask extends LaunchTask {
  kind: "webview";
}

export const launchPreviewCompat = async (task: LaunchInBrowserTask | LaunchInWebViewTask) => {
  let shadowDispose: vscode.Disposable | undefined = undefined;
  let shadowDisposeClose: vscode.Disposable | undefined = undefined;
  const { context, editor: activeEditor, bindDocument, webviewPanel } = task;
  const filePath = bindDocument.uri.fsPath;

  const refreshStyle =
    vscode.workspace.getConfiguration().get<string>("typst-preview.refresh") || "onSave";
  const scrollSyncMode =
    ScrollSyncModeEnum[
      vscode.workspace.getConfiguration().get<ScrollSyncMode>("typst-preview.scrollSync") || "never"
    ];
  const enableCursor =
    vscode.workspace.getConfiguration().get<boolean>("typst-preview.cursorIndicator") || false;
  await watchEditorFiles();
  const { serverProcess, controlPlanePort, dataPlanePort, staticFilePort } = await launchCli(
    task.kind === "browser",
  );

  const addonΠserver = new Addon2Server(
    controlPlanePort,
    enableCursor,
    scrollSyncMode,
    bindDocument,
    activeEditor,
  );

  // interact with typst-lsp
  if (vscode.workspace.getConfiguration().get<boolean>("typst-preview.pinPreviewFile")) {
    console.log("pinPreviewFile");
    vscode.commands.executeCommand("typst-lsp.pinMainToCurrent");
  }

  serverProcess.on("exit", (_code: any) => {
    if (activeTask.has(bindDocument)) {
      activeTask.delete(bindDocument);
    }
    addonΠserver.dispose();
    shadowDispose?.dispose();
    shadowDisposeClose?.dispose();

    // interact with typst-lsp
    if (vscode.workspace.getConfiguration().get<boolean>("typst-preview.pinPreviewFile")) {
      vscode.commands.executeCommand("typst-lsp.unpinMain");
    }
  });

  const connectUrl = `ws://127.0.0.1:${dataPlanePort}`;
  contentPreviewProvider.then((p) => p.postActivate(connectUrl));
  let panel: vscode.WebviewPanel | undefined = undefined;
  if (task.kind == "webview") {
    panel = await openPreviewInWebView({
      context,
      task,
      activeEditor,
      dataPlanePort,
      webviewPanel,
      panelDispose() {
        activeTask.delete(bindDocument);
        serverProcess.kill();
        contentPreviewProvider.then((p) => p.postDeactivate(connectUrl));
      },
    });
  }
  // todo: may override the same file
  activeTask.set(bindDocument, {
    panel,
    addonΠserver,
    staticFilePort,
  });

  return { message: "ok" };

  async function watchEditorFiles() {
    if (refreshStyle === "onType") {
      console.log("watch editor changes");

      shadowDispose = vscode.workspace.onDidChangeTextDocument(async (e) => {
        if (e.document.uri.scheme === "file") {
          // console.log("... ", "updateMemoryFiles", e.document.fileName);
          addonΠserver.conn.send(
            JSON.stringify({
              event: "updateMemoryFiles",
              files: {
                [e.document.fileName]: e.document.getText(),
              },
            }),
          );
        }
      });
      shadowDisposeClose = vscode.workspace.onDidSaveTextDocument(async (e) => {
        if (e.uri.scheme === "file") {
          console.log("... ", "saveMemoryFiles", e.fileName);
          addonΠserver.conn.send(
            JSON.stringify({
              event: "removeMemoryFiles",
              files: [e.fileName],
            }),
          );
        }
      });
    }
  }

  async function launchCli(openInBrowser: boolean) {
    const serverPath = getCliPath();
    console.log(`Watching ${filePath} for changes, using ${serverPath} as server`);
    const projectRoot = getProjectRoot(filePath);
    const rootArgs = ["--root", projectRoot];
    const partialRenderingArgs = vscode.workspace
      .getConfiguration()
      .get<boolean>("typst-preview.partialRendering")
      ? ["--partial-rendering"]
      : [];
    const ivArgs = vscode.workspace.getConfiguration().get<string>("typst-preview.invertColors");
    const invertColorsArgs = ivArgs ? ["--invert-colors", ivArgs] : [];
    const previewInSlideModeArgs = task.mode === "slide" ? ["--preview-mode=slide"] : [];
    const { dataPlanePort, controlPlanePort, staticFilePort, serverProcess } = await runServer(
      serverPath,
      projectRoot,
      [
        "preview",
        "--data-plane-host",
        "127.0.0.1:0",
        "--control-plane-host",
        "127.0.0.1:0",
        "--no-open",
        ...rootArgs,
        ...partialRenderingArgs,
        ...invertColorsArgs,
        ...previewInSlideModeArgs,
        ...codeGetCliInputArgs(),
        ...codeGetCliFontArgs(),
        filePath,
      ],
      outputChannel!,
    );
    console.log(
      `Launched server, data plane port:${dataPlanePort}, control plane port:${controlPlanePort}, static file port:${staticFilePort}`,
    );
    if (openInBrowser) {
      vscode.env.openExternal(vscode.Uri.parse(`http://127.0.0.1:${staticFilePort}`));
    }
    // window.typstWebsocket.send("current");
    return {
      serverProcess,
      dataPlanePort,
      controlPlanePort,
      staticFilePort,
    };
  }
};

function getProjectRoot(currentPath: string): string {
  const checkIfPathContains = (base: string, target: string) => {
    const relativePath = path.relative(base, target);
    return !relativePath.startsWith("..") && !path.isAbsolute(relativePath);
  };
  const paths = vscode.workspace.workspaceFolders
    ?.map((folder) => folder.uri.fsPath)
    .filter((folder) => checkIfPathContains(folder, currentPath));
  if (!paths || paths.length === 0) {
    // return path's parent folder
    return path.dirname(currentPath);
  } else {
    return paths[0];
  }
}

function getCliInputArgs(inputs?: { [key: string]: string }): string[] {
  return Object.entries(inputs ?? {})
    .filter(([k, _]) => k.trim() !== "")
    .map(([k, v]) => ["--input", `${k}=${v}`])
    .flat();
}

export function codeGetCliInputArgs(): string[] {
  return getCliInputArgs(
    vscode.workspace.getConfiguration().get<{ [key: string]: string }>("typst-preview.sysInputs"),
  );
}

export function getCliFontPathArgs(fontPaths?: string[]): string[] {
  return (fontPaths ?? []).flatMap((fontPath) => ["--font-path", vscodeVariables(fontPath)]);
}

export function codeGetCliFontArgs(): string[] {
  const needSystemFonts = vscode.workspace
    .getConfiguration()
    .get<boolean>("typst-preview.systemFonts");
  const fontPaths = getCliFontPathArgs(
    vscode.workspace.getConfiguration().get<string[]>("typst-preview.fontPaths"),
  );
  return [...(needSystemFonts ? [] : ["--ignore-system-fonts"]), ...fontPaths];
}

export class Addon2Server {
  disposes: vscode.Disposable[] = [];
  conn: WebSocket;

  constructor(
    controlPlanePort: string,
    enableCursor: boolean,
    scrollSyncMode: ScrollSyncModeEnum,
    bindDocument: vscode.TextDocument,
    activeEditor: vscode.TextEditor,
  ) {
    const conn = new WebSocket(`ws://127.0.0.1:${controlPlanePort}`);
    conn.addEventListener("message", async (message) => {
      const data = JSON.parse(message.data as string);
      switch (data.event) {
        case "editorScrollTo":
          return await editorScrollTo(activeEditor, data as JumpInfo);
        case "syncEditorChanges":
          return syncEditorChanges(conn);
        case "compileStatus": {
          statusBarItemProcess(data.kind as "Compiling" | "CompileSuccess" | "CompileError");
          break;
        }
        case "outline": {
          previewProcessOutline(data);
          break;
        }
        default: {
          console.warn("unknown message", data);
          break;
        }
      }
    });

    if (enableCursor) {
      conn.addEventListener("open", () => {
        reportPosition(bindDocument, activeEditor, "changeCursorPosition");
      });
    }

    if (scrollSyncMode !== ScrollSyncModeEnum.never) {
      // See comment of reportPosition function to get context about multi-file project related logic.
      const src2docHandler = (e: vscode.TextEditorSelectionChangeEvent) => {
        if (e.textEditor === activeEditor || activeTask.size === 1) {
          const editor = e.textEditor === activeEditor ? activeEditor : e.textEditor;
          const doc = e.textEditor === activeEditor ? bindDocument : e.textEditor.document;

          const kind = e.kind;
          console.log(
            `selection changed, kind: ${kind && vscode.TextEditorSelectionChangeKind[kind]}`,
          );
          const shouldScrollPanel =
            // scroll by mouse
            kind === vscode.TextEditorSelectionChangeKind.Mouse ||
            // scroll by keyboard typing
            (scrollSyncMode === ScrollSyncModeEnum.onSelectionChange &&
              kind === vscode.TextEditorSelectionChangeKind.Keyboard);
          if (shouldScrollPanel) {
            console.log(`selection changed, sending src2doc jump request`);
            reportPosition(doc, editor, "panelScrollTo");
          }

          if (enableCursor) {
            reportPosition(doc, editor, "changeCursorPosition");
          }
        }
      };

      this.disposes.push(vscode.window.onDidChangeTextEditorSelection(src2docHandler, 500));
    }

    this.conn = conn;

    interface JumpInfo {
      filepath: string;
      start: [number, number] | null;
      end: [number, number] | null;
    }

    async function editorScrollTo(activeEditor: vscode.TextEditor, jump: JumpInfo) {
      console.log("recv editorScrollTo request", jump);
      if (jump.start === null || jump.end === null) {
        return;
      }

      // open this file and show in editor
      const doc = await vscode.workspace.openTextDocument(jump.filepath);
      const editor = await vscode.window.showTextDocument(doc, activeEditor.viewColumn);
      const startPosition = new vscode.Position(jump.start[0], jump.start[1]);
      const endPosition = new vscode.Position(jump.end[0], jump.end[1]);
      const range = new vscode.Range(startPosition, endPosition);
      editor.selection = new vscode.Selection(range.start, range.end);
      editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
    }

    function syncEditorChanges(addonΠserver: WebSocket) {
      console.log("recv syncEditorChanges request");
      const files: Record<string, string> = {};
      vscode.workspace.textDocuments.forEach((doc) => {
        if (doc.isDirty) {
          files[doc.fileName] = doc.getText();
        }
      });

      addonΠserver.send(
        JSON.stringify({
          event: "syncMemoryFiles",
          files,
        }),
      );
    }
  }

  dispose() {
    this.disposes.forEach((d) => d.dispose());
    this.conn.close();
  }
}

interface SourceScrollBySpanRequest {
  event: "sourceScrollBySpan";
  span: string;
}

interface ScrollByPositionRequest {
  event: "panelScrollByPosition";
  position: any;
}

interface ScrollRequest {
  event: string;
  filepath: string;
  line: any;
  character: any;
}

type DocRequests = SourceScrollBySpanRequest | ScrollByPositionRequest | ScrollRequest;

// If there is only one preview task, we treat the workspace as a multi-file project,
// so `Sync preview with cursor` command in any file goes to the unique preview server.
//
// If there are more then one preview task, we assume user is previewing serval single file
// document, only process sync command directly happened in those file.
//
// This is a compromise we made to support multi-file projects after evaluating performance,
// effectiveness, and user needs.
// See https://github.com/Enter-tainer/typst-preview/issues/164 for more detail.
const sendDocRequest = async (
  bindDocument: vscode.TextDocument | undefined,
  scrollRequest: DocRequests,
) => {
  let tcb = bindDocument && activeTask.get(bindDocument);
  if (tcb === undefined) {
    if (activeTask.size === 1) {
      tcb = Array.from(activeTask.values())[0];
    } else {
      return;
    }
  }
  tcb.addonΠserver.conn.send(JSON.stringify(scrollRequest));
};

const reportPosition = async (
  bindDocument: vscode.TextDocument,
  activeEditor: vscode.TextEditor,
  event: string,
) => {
  // extension-output
  if (bindDocument.uri.fsPath.includes("extension-output")) {
    console.log("skip extension-output file", bindDocument.uri.fsPath);
    return;
  }

  const scrollRequest: ScrollRequest = {
    event,
    filepath: bindDocument.uri.fsPath,
    line: activeEditor.selection.active.line,
    character: activeEditor.selection.active.character,
  };
  // console.log(scrollRequest);
  sendDocRequest(bindDocument, scrollRequest);
};

export const panelSyncScrollCompat = async () => {
  const activeEditor = vscode.window.activeTextEditor;
  if (!activeEditor) {
    vscode.window.showWarningMessage("No active editor");
    return;
  }

  reportPosition(activeEditor.document, activeEditor, "panelScrollTo");
};

export const revealDocumentCompat = async (args: any) => {
  console.log(args);
  // That's very unfortunate that sourceScrollBySpan doesn't work well.
  if (args.span) {
    sendDocRequest(undefined, {
      event: "sourceScrollBySpan",
      span: args.span,
    });
  }
  if (args.position) {
    // todo: tagging document
    sendDocRequest(undefined, {
      event: "panelScrollByPosition",
      position: args.position,
    });
  }
};

export const ejectPreviewPanelCompat = async () => {
  vscode.window.showWarningMessage("Eject is not supported in compat mode");
}
