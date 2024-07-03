// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import * as path from "path";
import { getServer } from "./extension";
import { ChildProcessWithoutNullStreams } from "child_process";
import { spawn } from "cross-spawn";
import { WebSocket } from "ws";
import type fetchFunc from "node-fetch";
import {
    ScrollSyncMode,
    ScrollSyncModeEnum,
    contentPreviewProvider,
    launchPreviewInWebView,
    previewProcessOutline,
} from "./preview";

const vscodeVariables = require("vscode-variables");

/// kill the probe task after 60s
const PROBE_TIMEOUT = 60_000;
let isTinymist = false;
let tinymistServerConfig: string | undefined;
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

export async function setIsTinymist(config: Record<string, any>) {
    isTinymist = true;
    tinymistServerConfig = config.server;
    guy = "$(sync)";
}

async function getCliPath(extensionPath?: string): Promise<string> {
    if (tinymistServerConfig) {
        return getServer(tinymistServerConfig!);
    }

    const { sync: spawnSync } = require("cross-spawn");

    const state = getCliPath as unknown as any;
    !state.BINARY_NAME && (state.BINARY_NAME = "tinymist");
    !state.getConfig &&
        (state.getConfig = () =>
            vscode.workspace.getConfiguration().get<string>("typst-preview.executable"));

    const bundledPath = path.resolve(
        extensionPath || path.join(__dirname, ".."),
        "out",
        state.BINARY_NAME
    );
    const configPath = state.getConfig();

    if (state.bundledPath === bundledPath && state.configPath === configPath) {
        // console.log('getCliPath cached', state.resolved);
        return state.resolved;
    }
    state.bundledPath = bundledPath;
    state.configPath = configPath;

    const checkExecutable = (path: string): string | null => {
        const child = spawnSync(path, ["-V"], {
            timeout: PROBE_TIMEOUT,
            encoding: "utf8",
        });
        if (child.error) {
            return child.error.message;
        }
        if (child.status !== 0) {
            return `exit code ${child.status}`;
        }
        // if (child.stdout.trim() !== `${name} ${version}`) {
        //     return `version mismatch, expected ${name} ${version}, got ${child.stdout}`;
        // }
        return null;
    };

    const resolvePath = async () => {
        console.log("getCliPath resolving", bundledPath, configPath);

        if (configPath?.length) {
            return configPath;
        }
        const errorMessage = checkExecutable(bundledPath);
        if (errorMessage === null) {
            return bundledPath;
        }
        vscode.window.showWarningMessage(
            `${state.BINARY_NAME} executable at ${bundledPath} not working,` +
                `maybe we didn't ship it for your platform or it cannot run due to library issues?` +
                `In this case you need compile and add ${state.BINARY_NAME} to your PATH.` +
                `Error: ${errorMessage}`
        );
        return state.BINARY_NAME;
    };

    return (state.resolved = await resolvePath());
}

export function statusBarInit() {
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
    statusBarItem.name = "typst-preview";
    statusBarItem.command = "typst-preview.showLog";
    statusBarItem.tooltip = "Typst Preview Status: Click to show logs";
    return statusBarItem;
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
        })
    );
    process.on("SIGINT", () => {
        for (const serverProcess of serverProcesses) {
            serverProcess.kill();
        }
    });

    let fetch: typeof fetchFunc | undefined = undefined;
    context.subscriptions.push(
        vscode.commands.registerCommand("typst-preview.showAwaitTree", async () => {
            if (activeTask.size === 0) {
                vscode.window.showWarningMessage("No active preview");
                return;
            }
            const showAwaitTree = async (tcb: TaskControlBlock) => {
                fetch = fetch || (await import("node-fetch")).default;

                const url = `http://127.0.0.1:${tcb.staticFilePort}/await_tree`;
                // fetch await tree
                const awaitTree = await (await fetch(`${url}`)).text();
                console.log(awaitTree);
                const input = await vscode.window.showInformationMessage(
                    "Click to copy the await tree to clipboard",
                    "Copy"
                );
                if (input === "Copy") {
                    vscode.env.clipboard.writeText(awaitTree);
                }
            };
            if (activeTask.size === 1) {
                await showAwaitTree(Array.from(activeTask.values())[0]);
            }
            const activeDocument = vscode.window.activeTextEditor?.document;
            if (activeDocument) {
                const task = activeTask.get(activeDocument);
                if (task) {
                    await showAwaitTree(task);
                }
            }
        })
    );
}

// This method is called when your extension is deactivated
export function previewDeactivateCompat() {
    console.log(activeTask);
    for (const [_, task] of activeTask) {
        task.panel?.dispose();
    }
    console.log("killing preview services");
    for (const serverProcess of serverProcesses) {
        serverProcess.kill();
    }
}

function statusBarItemProcess(event: "Compiling" | "CompileSuccess" | "CompileError") {
    // if (isTinymist) {
    //     return;
    // }

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
            statusBarItem.backgroundColor = new vscode.ThemeColor(
                "statusBarItem.prominentBackground"
            );
            statusBarItem.show();
        } else if (event === "CompileSuccess") {
            if (style === "compact") {
                statusBarItem.text = `${guy}`;
            } else if (style === "full") {
                statusBarItem.text = `${guy} Compile Success`;
            }
            statusBarItem.backgroundColor = new vscode.ThemeColor(
                "statusBarItem.prominentBackground"
            );
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

let statusBarItem: vscode.StatusBarItem;

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
    outputChannel: vscode.OutputChannel
): Promise<LaunchCliResult> {
    const serverProcess = spawn(command, args, {
        env: {
            ...process.env,
            // eslint-disable-next-line @typescript-eslint/naming-convention
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
                "Show Logs"
            );
            if (response === "Show Logs") {
                outputChannel.show();
            }
        }
        console.log(`child process exited with code ${code}`);
    });

    serverProcesses.push(serverProcesses);
    return new Promise((resolve, reject) => {
        let dataPlanePort: string | undefined = undefined;
        let controlPlanePort: string | undefined = undefined;
        let staticFilePort: string | undefined = undefined;
        serverProcess.stderr.on("data", (data: Buffer) => {
            if (data.toString().includes("listening on")) {
                console.log(data.toString());
                let ctrlPort = data
                    .toString()
                    .match(/Control plane server listening on: 127\.0\.0\.1:(\d+)/)?.[1];
                let dataPort = data
                    .toString()
                    .match(/Data plane server listening on: 127\.0\.0\.1:(\d+)/)?.[1];
                let staticPort = data
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
    activeEditor: vscode.TextEditor;
    bindDocument: vscode.TextDocument;
    mode: "doc" | "slide";
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
    const { context, activeEditor, bindDocument } = task;
    const filePath = bindDocument.uri.fsPath;

    const refreshStyle =
        vscode.workspace.getConfiguration().get<string>("typst-preview.refresh") || "onSave";
    const scrollSyncMode =
        ScrollSyncModeEnum[
            vscode.workspace.getConfiguration().get<ScrollSyncMode>("typst-preview.scrollSync") ||
                "never"
        ];
    const enableCursor =
        vscode.workspace.getConfiguration().get<boolean>("typst-preview.cursorIndicator") || false;
    await watchEditorFiles();
    const { serverProcess, controlPlanePort, dataPlanePort, staticFilePort } = await launchCli(
        task.kind === "browser"
    );

    const addonΠserver = new Addon2Server(
        controlPlanePort,
        enableCursor,
        scrollSyncMode,
        bindDocument,
        activeEditor
    );

    // interact with typst-lsp
    if (vscode.workspace.getConfiguration().get<boolean>("typst-preview.pinPreviewFile")) {
        console.log("pinPreviewFile");
        vscode.commands.executeCommand("typst-lsp.pinMainToCurrent");
    }

    serverProcess.on("exit", (code: any) => {
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

    let connectUrl = `ws://127.0.0.1:${dataPlanePort}`;
    contentPreviewProvider.then((p) => p.postActivate(connectUrl));
    let panel: vscode.WebviewPanel | undefined = undefined;
    if (task.kind == "webview") {
        panel = await launchPreviewInWebView({
            context,
            task,
            activeEditor,
            dataPlanePort,
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
                        })
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
                        })
                    );
                }
            });
        }
    }

    async function launchCli(openInBrowser: boolean) {
        const serverPath = await getCliPath(context.extensionPath);
        console.log(`Watching ${filePath} for changes, using ${serverPath} as server`);
        const projectRoot = getProjectRoot(filePath);
        const rootArgs = ["--root", projectRoot];
        const partialRenderingArgs = vscode.workspace
            .getConfiguration()
            .get<boolean>("typst-preview.partialRendering")
            ? ["--partial-rendering"]
            : [];
        const ivArgs = vscode.workspace
            .getConfiguration()
            .get<string>("typst-preview.invertColors");
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
                "--static-file-host",
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
            outputChannel!
        );
        console.log(
            `Launched server, data plane port:${dataPlanePort}, control plane port:${controlPlanePort}, static file port:${staticFilePort}`
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
        vscode.workspace
            .getConfiguration()
            .get<{ [key: string]: string }>("typst-preview.sysInputs")
    );
}

export function getCliFontPathArgs(fontPaths?: string[]): string[] {
    return (fontPaths ?? []).flatMap((fontPath) => ["--font-path", vscodeVariables(fontPath)]);
}

export function codeGetCliFontArgs(): string[] {
    let needSystemFonts = vscode.workspace
        .getConfiguration()
        .get<boolean>("typst-preview.systemFonts");
    let fontPaths = getCliFontPathArgs(
        vscode.workspace.getConfiguration().get<string[]>("typst-preview.fontPaths")
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
        activeEditor: vscode.TextEditor
    ) {
        const conn = new WebSocket(`ws://127.0.0.1:${controlPlanePort}`);
        conn.addEventListener("message", async (message) => {
            const data = JSON.parse(message.data as string);
            switch (data.event) {
                case "editorScrollTo":
                    return await editorScrollTo(activeEditor, data /* JumpInfo */);
                case "syncEditorChanges":
                    return syncEditorChanges(conn);
                case "compileStatus": {
                    statusBarItemProcess(data.kind);
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
                    const doc =
                        e.textEditor === activeEditor ? bindDocument : e.textEditor.document;

                    const kind = e.kind;
                    console.log(
                        `selection changed, kind: ${kind && vscode.TextEditorSelectionChangeKind[kind]}`
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
            let files: Record<string, string> = {};
            vscode.workspace.textDocuments.forEach((doc) => {
                if (doc.isDirty) {
                    files[doc.fileName] = doc.getText();
                }
            });

            addonΠserver.send(
                JSON.stringify({
                    event: "syncMemoryFiles",
                    files,
                })
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
    scrollRequest: DocRequests
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
    event: string
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
