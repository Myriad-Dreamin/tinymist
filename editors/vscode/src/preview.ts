// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import * as path from "path";
import { DisposeList, getTargetViewColumn, loadHTMLFile } from "./util";
import {
    launchPreviewCompat,
    previewActiveCompat as previewPostActivateCompat,
    previewDeactivateCompat,
    revealDocumentCompat,
    panelSyncScrollCompat,
    LaunchInWebViewTask,
    LaunchInBrowserTask,
    getPreviewHtml as getPreviewHtmlCompat,
} from "./preview-compat";
import {
    commandKillPreview,
    commandScrollPreview,
    commandStartPreview,
    registerPreviewTaskDispose,
} from "./extension";

export function previewPreload(context: vscode.ExtensionContext) {
    getPreviewHtmlCompat(context);
}

let launchImpl: typeof launchPreviewLsp;
export function previewActivate(context: vscode.ExtensionContext, isCompat: boolean) {
    // https://github.com/microsoft/vscode-extension-samples/blob/4721ef0c450f36b5bce2ecd5be4f0352ed9e28ab/webview-view-sample/src/extension.ts#L3
    getPreviewHtmlCompat(context).then((html) => {
        if (!html) {
            vscode.window.showErrorMessage("Failed to load content preview content");
            return;
        }
        const provider = new ContentPreviewProvider(context, context.extensionUri, html);
        resolveContentPreviewProvider(provider);
        context.subscriptions.push(
            vscode.window.registerWebviewViewProvider(
                isCompat ? "typst-preview.content-preview" : "tinymist.preview.content-preview",
                provider
            )
        );
    });
    {
        const outlineProvider = new OutlineProvider(context.extensionUri);
        resolveOutlineProvider(outlineProvider);
        context.subscriptions.push(
            vscode.window.registerTreeDataProvider(
                isCompat ? "typst-preview.outline" : "tinymist.preview.outline",
                outlineProvider
            )
        );
    }
    context.subscriptions.push(
        vscode.window.registerWebviewPanelSerializer(
            "typst-preview",
            new TypstPreviewSerializer(context)
        )
    );

    context.subscriptions.push(
        vscode.commands.registerCommand("typst-preview.preview", launch("webview", "doc")),
        vscode.commands.registerCommand("typst-preview.browser", launch("browser", "doc")),
        vscode.commands.registerCommand("typst-preview.preview-slide", launch("webview", "slide")),
        vscode.commands.registerCommand("typst-preview.browser-slide", launch("browser", "slide"))
    );
    context.subscriptions.push(
        vscode.commands.registerCommand(
            "typst-preview.revealDocument",
            isCompat ? revealDocumentCompat : revealDocumentLsp
        )
    );
    context.subscriptions.push(
        vscode.commands.registerCommand(
            "typst-preview.sync",
            isCompat ? panelSyncScrollCompat : panelSyncScrollLsp
        )
    );

    if (isCompat) {
        previewPostActivateCompat(context);
    }

    launchImpl = isCompat ? launchPreviewCompat : launchPreviewLsp;
    function launch(kind: "browser" | "webview", mode: "doc" | "slide") {
        return async () => {
            const activeEditor = vscode.window.activeTextEditor;
            if (!activeEditor) {
                vscode.window.showWarningMessage("No active editor");
                return;
            }
            const bindDocument = activeEditor.document;
            return launchImpl({
                kind,
                context,
                activeEditor,
                bindDocument,
                mode,
            }).catch((e) => {
                vscode.window.showErrorMessage(`failed to launch preview: ${e}`);
            });
        };
    }
}

// This method is called when your extension is deactivated
export function previewDeactivate() {
    previewDeactivateCompat();
}

function getPreviewConfCompat<T>(s: string) {
    const t = vscode.workspace.getConfiguration().get<T>(`tinymist.preview.${s}`);
    if (t !== undefined) {
        return t;
    }

    return vscode.workspace.getConfiguration().get<T>(`typst-preview.${s}`);
}

export async function launchPreviewInWebView({
    context,
    task,
    activeEditor,
    dataPlanePort,
    webviewPanel,
    panelDispose,
}: {
    context: vscode.ExtensionContext;
    task: LaunchInWebViewTask;
    activeEditor: vscode.TextEditor;
    dataPlanePort: string | number;
    webviewPanel?: vscode.WebviewPanel;
    panelDispose: () => void;
}) {
    const basename = path.basename(activeEditor.document.fileName);
    const fontendPath = path.resolve(context.extensionPath, "out/frontend");
    // Create and show a new WebView
    const panel =
        webviewPanel !== undefined
            ? webviewPanel
            : vscode.window.createWebviewPanel(
                  "typst-preview", // 标识符
                  `${basename} (Preview)`, // 面板标题
                  getTargetViewColumn(activeEditor.viewColumn),
                  {
                      enableScripts: true, // 启用 JS
                      retainContextWhenHidden: true,
                  }
              );

    // todo: bindDocument.onDidDispose, but we did not find a similar way.
    panel.onDidDispose(async () => {
        panelDispose();
        console.log("killed preview services");
        panel.dispose();
    });

    // 将已经准备好的 HTML 设置为 Webview 内容
    let html = await getPreviewHtmlCompat(context);
    html = html.replace(
        /\/typst-webview-assets/g,
        `${panel.webview
            .asWebviewUri(vscode.Uri.file(fontendPath))
            .toString()}/typst-webview-assets`
    );
    const previewMode = task.mode === "doc" ? "Doc" : "Slide";
    const previewState = { mode: task.mode, fsPath: activeEditor.document.uri.fsPath };
    const previewStateEncoded = Buffer.from(JSON.stringify(previewState), "utf-8").toString(
        "base64"
    );
    html = html.replace("preview-arg:previewMode:Doc", `preview-arg:previewMode:${previewMode}`);
    html = html.replace("preview-arg:state:", `preview-arg:state:${previewStateEncoded}`);

    panel.webview.html = html.replace("ws://127.0.0.1:23625", `ws://127.0.0.1:${dataPlanePort}`);
    // 虽然配置的是 http，但是如果是桌面客户端，任何 tcp 连接都支持，这也就包括了 ws
    // https://code.visualstudio.com/api/advanced-topics/remote-extensions#forwarding-localhost
    await vscode.env.asExternalUri(vscode.Uri.parse(`http://127.0.0.1:${dataPlanePort}`));
    return panel;
}

interface TaskControlBlock {
    /// related panel
    panel?: vscode.WebviewPanel;
    /// random task id
    taskId: string;
}
const activeTask = new Map<vscode.TextDocument, TaskControlBlock>();

async function launchPreviewLsp(task: LaunchInBrowserTask | LaunchInWebViewTask) {
    const { kind, context, activeEditor, bindDocument } = task;
    if (activeTask.has(bindDocument)) {
        const { panel } = activeTask.get(bindDocument)!;
        if (panel) {
            panel.reveal();
        }
        return;
    }

    const taskId = Math.random().toString(36).substring(7);
    const filePath = bindDocument.uri.fsPath;

    const refreshStyle = getPreviewConfCompat<string>("refresh") || "onSave";
    const scrollSyncMode =
        ScrollSyncModeEnum[getPreviewConfCompat<ScrollSyncMode>("scrollSync") || "never"];
    const enableCursor = getPreviewConfCompat<boolean>("cursorIndicator") || false;
    const disposes = new DisposeList();
    registerPreviewTaskDispose(taskId, disposes);
    const { dataPlanePort, staticServerPort } = await launchCommand();
    if (!dataPlanePort || !staticServerPort) {
        disposes.dispose();
        throw new Error(`Failed to launch preview ${filePath}`);
    }

    let connectUrl = `ws://127.0.0.1:${dataPlanePort}`;
    contentPreviewProvider.then((p) => p.postActivate(connectUrl));
    disposes.add(() => {
        contentPreviewProvider.then((p) => p.postDeactivate(connectUrl));
    });

    let panel: vscode.WebviewPanel | undefined = undefined;
    switch (kind) {
        case "webview": {
            panel = await launchPreviewInWebView({
                context,
                task,
                activeEditor,
                dataPlanePort,
                panelDispose() {
                    disposes.dispose();
                    commandKillPreview(taskId);
                },
            });
            break;
        }
        case "browser": {
            vscode.env.openExternal(vscode.Uri.parse(`http://127.0.0.1:${staticServerPort}`));
            break;
        }
    }

    // todo: may override the same file
    // todo: atomic update
    activeTask.set(bindDocument, {
        panel,
        taskId,
    });
    disposes.add(() => {
        if (activeTask.get(bindDocument)?.taskId === taskId) {
            activeTask.delete(bindDocument);
        }

        // todo: better way to unpin main
        vscode.commands.executeCommand("tinymist.unpinMain");
    });

    async function launchCommand() {
        console.log(`Preview Command ${filePath}`);
        const partialRenderingArgs = getPreviewConfCompat<boolean>("partialRendering")
            ? ["--partial-rendering"]
            : [];
        const ivArgs = getPreviewConfCompat<string>("invertColors");
        const invertColorsArgs = ivArgs ? ["--invert-colors", ivArgs] : [];
        const previewInSlideModeArgs = task.mode === "slide" ? ["--preview-mode=slide"] : [];
        const { dataPlanePort, staticServerPort } = await commandStartPreview([
            "--task-id",
            taskId,
            "--refresh-style",
            refreshStyle,
            "--data-plane-host",
            "127.0.0.1:0",
            "--static-file-host",
            "127.0.0.1:0",
            ...partialRenderingArgs,
            ...invertColorsArgs,
            ...previewInSlideModeArgs,
            filePath,
        ]);
        console.log(
            `Launched preview, data plane port:${dataPlanePort}, static server port:${staticServerPort}`
        );

        if (enableCursor) {
            reportPosition(activeEditor, "changeCursorPosition");
        }

        if (scrollSyncMode !== ScrollSyncModeEnum.never) {
            // See comment of reportPosition function to get context about multi-file project related logic.
            const src2docHandler = (e: vscode.TextEditorSelectionChangeEvent) => {
                const editor = e.textEditor;
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
                    reportPosition(editor, "panelScrollTo");
                }

                if (enableCursor) {
                    reportPosition(editor, "changeCursorPosition");
                }
            };

            disposes.add(vscode.window.onDidChangeTextEditorSelection(src2docHandler, 500));
        }

        return { staticServerPort, dataPlanePort };
    }

    async function reportPosition(
        editorToReport: vscode.TextEditor,
        event: "changeCursorPosition" | "panelScrollTo"
    ) {
        const scrollRequest: ScrollRequest = {
            event,
            filepath: editorToReport.document.uri.fsPath,
            line: editorToReport.selection.active.line,
            character: editorToReport.selection.active.character,
        };
        scrollPreviewPanel(taskId, scrollRequest);
    }
}

async function revealDocumentLsp(args: any) {
    console.log("revealDocumentLsp", args);

    for (const t of activeTask.values()) {
        if (args.taskId && t.taskId !== args.taskId) {
            return;
        }

        if (args.span) {
            // That's very unfortunate that sourceScrollBySpan doesn't work well.
            scrollPreviewPanel(t.taskId, {
                event: "sourceScrollBySpan",
                span: args.span,
            });
        }
        if (args.position) {
            // todo: tagging document
            scrollPreviewPanel(t.taskId, {
                event: "panelScrollByPosition",
                position: args.position,
            });
        }
    }
}

async function panelSyncScrollLsp(args: any) {
    const activeEditor = vscode.window.activeTextEditor;
    if (!activeEditor) {
        vscode.window.showWarningMessage("No active editor");
        return;
    }

    for (const t of activeTask.values()) {
        if (args.taskId && t.taskId !== args.taskId) {
            return;
        }

        const scrollRequest: ScrollRequest = {
            event: "panelScrollTo",
            filepath: activeEditor.document.uri.fsPath,
            line: activeEditor.selection.active.line,
            character: activeEditor.selection.active.character,
        };
        scrollPreviewPanel(t.taskId, scrollRequest);
    }
}

// That's very unfortunate that sourceScrollBySpan doesn't work well.
interface SourceScrollBySpanRequest {
    event: "sourceScrollBySpan";
    span: string;
}

interface ScrollByPositionRequest {
    event: "panelScrollByPosition";
    position: any;
}

interface ScrollRequest {
    event: "panelScrollTo" | "changeCursorPosition";
    filepath: string;
    line: any;
    character: any;
}

type DocRequests = SourceScrollBySpanRequest | ScrollByPositionRequest | ScrollRequest;

async function scrollPreviewPanel(taskId: string, scrollRequest: DocRequests) {
    if ("filepath" in scrollRequest) {
        const filepath = scrollRequest.filepath;
        if (filepath.includes("extension-output")) {
            console.log("skip extension-output file", filepath);
            return;
        }
    }

    commandScrollPreview(taskId, scrollRequest);
}

let resolveContentPreviewProvider: (value: ContentPreviewProvider) => void = () => {};
export let contentPreviewProvider = new Promise<ContentPreviewProvider>((resolve) => {
    resolveContentPreviewProvider = resolve;
});

let resolveOutlineProvider: (value: OutlineProvider) => void = () => {};
export let outlineProvider = new Promise<OutlineProvider>((resolve) => {
    resolveOutlineProvider = resolve;
});

export enum ScrollSyncModeEnum {
    never,
    onSelectionChangeByMouse,
    onSelectionChange,
}

export type ScrollSyncMode = "never" | "onSelectionChangeByMouse" | "onSelectionChange";

export function previewProcessOutline(outlineData: any) {
    contentPreviewProvider.then((p) => p.postOutlineItem(outlineData /* Outline */));
    outlineProvider.then((p) => p.postOutlineItem(outlineData /* Outline */));
}

class ContentPreviewProvider implements vscode.WebviewViewProvider {
    private _view?: vscode.WebviewView;

    constructor(
        private readonly context: vscode.ExtensionContext,
        private readonly extensionUri: vscode.Uri,
        private readonly htmlContent: string
    ) {}

    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ) {
        this._view = webviewView; // 将已经准备好的 HTML 设置为 Webview 内容

        const fontendPath = path.resolve(this.context.extensionPath, "out/frontend");
        let html = this.htmlContent.replace(
            /\/typst-webview-assets/g,
            `${this._view.webview
                .asWebviewUri(vscode.Uri.file(fontendPath))
                .toString()}/typst-webview-assets`
        );

        html = html.replace("ws://127.0.0.1:23625", ``);

        webviewView.webview.options = {
            // Allow scripts in the webview
            enableScripts: true,

            localResourceRoots: [this.extensionUri],
        };

        webviewView.webview.html = html;

        webviewView.webview.onDidReceiveMessage((data) => {
            switch (data.type) {
                case "started": {
                    // on content preview restarted
                    this.resetHost();
                    break;
                }
            }
        });
    }

    resetHost() {
        if (this._view && this.current) {
            console.log("postActivateSent", this.current);
            this._view.webview.postMessage(this.current);
        }
        if (this._view && this.currentOutline) {
            this._view.webview.postMessage(this.currentOutline);
            this.currentOutline = undefined;
        }
    }

    current: any = undefined;
    postActivate(url: string) {
        this.current = {
            type: "reconnect",
            url,
            mode: "Doc",
            isContentPreview: true,
        };
        this.resetHost();
    }

    postDeactivate(url: string) {
        if (this.current && this.current.url === url) {
            this.currentOutline = undefined;
            this.postActivate("");
        }
    }

    currentOutline: any = undefined;
    postOutlineItem(outline: any) {
        this.currentOutline = {
            type: "outline",
            outline,
            isContentPreview: true,
        };
        if (this._view) {
            this._view.webview.postMessage(this.currentOutline);
            this.currentOutline = undefined;
        }
    }
}

// todo: useful content security policy but we don't set
// Use a nonce to only allow a specific script to be run.
// const nonce = getNonce();

// function getNonce() {
// 	let text = '';
// 	const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
// 	for (let i = 0; i < 32; i++) {
// 		text += possible.charAt(Math.floor(Math.random() * possible.length));
// 	}
// 	return text;
// }

// <!--
// Use a content security policy to only allow loading styles from our extension directory,
// and only allow scripts that have a specific nonce.
// (See the 'webview-sample' extension sample for img-src content security policy examples)
// -->
// <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';">

interface CursorPosition {
    // eslint-disable-next-line @typescript-eslint/naming-convention
    page_no: number;
    x: number;
    y: number;
}

interface OutlineItemData {
    title: string;
    span?: string;
    position?: CursorPosition;
    children: OutlineItemData[];
}

class OutlineProvider implements vscode.TreeDataProvider<OutlineItem> {
    constructor(private readonly _extensionUri: vscode.Uri) {}

    private _onDidChangeTreeData: vscode.EventEmitter<OutlineItem | undefined | void> =
        new vscode.EventEmitter<OutlineItem | undefined | void>();
    readonly onDidChangeTreeData: vscode.Event<OutlineItem | undefined | void> =
        this._onDidChangeTreeData.event;

    refresh(): void {
        this._onDidChangeTreeData.fire();
    }

    outline: { items: OutlineItemData[] } | undefined = undefined;
    postOutlineItem(outline: any) {
        console.log("postOutlineItemProvider", outline);
        this.outline = outline;
        this.refresh();
    }

    getTreeItem(element: OutlineItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: OutlineItem): Thenable<OutlineItem[]> {
        if (!this.outline) {
            return Promise.resolve([]);
        }

        const children = (element ? element.data.children : this.outline.items) || [];
        return Promise.resolve(
            children.map((item: OutlineItemData) => {
                return new OutlineItem(
                    item,
                    item.children.length > 0
                        ? vscode.TreeItemCollapsibleState.Collapsed
                        : vscode.TreeItemCollapsibleState.None
                );
            })
        );
    }
}

export class OutlineItem extends vscode.TreeItem {
    constructor(
        public readonly data: OutlineItemData,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState,
        public readonly command: vscode.Command = {
            title: "Reveal Outline Item",
            command: "typst-preview.revealDocument",
            arguments: [{ span: data.span, position: data.position }],
        }
    ) {
        super(data.title, collapsibleState);
        const span = this.data.span;
        let detachedHint = span ? `` : `, detached`;

        const pos = this.data.position;
        if (pos) {
            this.tooltip = `${this.label} in page ${pos.page_no}, at (${pos.x.toFixed(3)} pt, ${pos.y.toFixed(3)} pt)${detachedHint}`;
            this.description = `page: ${pos.page_no}, at (${pos.x.toFixed(1)} pt, ${pos.y.toFixed(1)} pt)${detachedHint}`;
        } else {
            this.tooltip = `${this.label}${detachedHint}`;
            this.description = `no pos`;
        }
    }

    // iconPath = {
    // 	light: path.join(__filename, '..', '..', 'resources', 'light', 'dependency.svg'),
    // 	dark: path.join(__filename, '..', '..', 'resources', 'dark', 'dependency.svg')
    // };

    contextValue = "outline-item";
}

class TypstPreviewSerializer implements vscode.WebviewPanelSerializer {
    context: vscode.ExtensionContext;

    constructor(context: vscode.ExtensionContext) {
        this.context = context;
    }

    async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel, state: any) {
        const activeEditor = vscode.window.visibleTextEditors.find(
            (editor) => editor.document.uri.fsPath === state.fsPath
        );

        if (!activeEditor) {
            return;
        }

        const bindDocument = activeEditor.document;
        const mode = state.mode;

        launchImpl({
            kind: "webview",
            context: this.context,
            activeEditor,
            bindDocument,
            mode,
            webviewPanel,
        });
    }
}
