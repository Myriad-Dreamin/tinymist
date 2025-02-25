/// This file provides the typst document preview feature for vscode.

import * as vscode from "vscode";
import * as path from "path";
import {
  DisposeList,
  getSensibleTextEditorColumn,
  getTargetViewColumn,
  translateExternalURL,
} from "../util";
import {
  launchPreviewCompat,
  previewActiveCompat as previewPostActivateCompat,
  previewDeactivate as previewDeactivateCompat,
  revealDocumentCompat,
  panelSyncScrollCompat,
  LaunchInWebViewTask,
  LaunchInBrowserTask,
  getPreviewHtml,
} from "./preview-compat";
import {
  PanelScrollOrCursorMoveRequest,
  registerPreviewTaskDispose,
  ScrollPreviewRequest,
  tinymist,
} from "../lsp";

/**
 * The launch preview implementation which depends on `isCompat` of previewActivate.
 */
let launchImpl: typeof launchPreviewLsp;
/**
 * The active editor owning *typst language document* to track.
 */
let activeEditor: vscode.TextEditor | undefined;

/**
 * Preload the preview resources to reduce the latency of the first preview.
 * @param context The extension context.
 */
export function previewPreload(context: vscode.ExtensionContext) {
  getPreviewHtml(context);
}

/**
 * Activate the typst preview feature. This is the "the main entry" of the preview feature.
 *
 * @param context The extension context.
 * @param isCompat Whether the preview feature is activated in the old `mgt19937.typst-preview`
 * extension.
 */
export function previewActivate(context: vscode.ExtensionContext, isCompat: boolean) {
  // Provides `ContentView` (ContentPreviewProvider) at the sidebar, which is a list of thumbnail
  // images.
  getPreviewHtml(context).then((html) => {
    if (!html) {
      vscode.window.showErrorMessage("Failed to load content preview content");
      return;
    }
    const provider = new ContentPreviewProvider(context, context.extensionUri, html);
    resolveContentPreviewProvider(provider);
    context.subscriptions.push(
      vscode.window.registerWebviewViewProvider(
        isCompat ? "typst-preview.content-preview" : "tinymist.preview.content-preview",
        provider,
      ),
    );
  });
  // Provides `OutlineView` (OutlineProvider) at the sidebar, which provides same content as the
  // exported PDF outline.
  {
    const outlineProvider = new OutlineProvider(context.extensionUri);
    resolveOutlineProvider(outlineProvider);
    context.subscriptions.push(
      vscode.window.registerTreeDataProvider(
        isCompat ? "typst-preview.outline" : "tinymist.preview.outline",
        outlineProvider,
      ),
    );
  }
  // Provides the `typst-preview` webview panel serializer to restore the preview state from last
  // vscode session.
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer(
      "typst-preview",
      new TypstPreviewSerializer(context),
    ),
  );

  // Tracks the active editor owning *typst language document*.
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor: vscode.TextEditor | undefined) => {
      const langId = editor?.document.languageId;
      if (langId === "typst") {
        activeEditor = editor;
      } else if (editor === undefined || activeEditor?.document.isClosed) {
        activeEditor = undefined;
      }
    }),
  );

  const launchBrowsingPreview = launch("webview", "doc", { isBrowsing: true });
  const launchDevPreview = launch("webview", "doc", { isDev: true });
  // Registers preview commands, check `package.json` for descriptions.
  context.subscriptions.push(
    vscode.commands.registerCommand("tinymist.browsing-preview", launchBrowsingPreview),
    vscode.commands.registerCommand("typst-preview.preview", launch("webview", "doc")),
    vscode.commands.registerCommand("typst-preview.browser", launch("browser", "doc")),
    vscode.commands.registerCommand("typst-preview.preview-slide", launch("webview", "slide")),
    vscode.commands.registerCommand("typst-preview.browser-slide", launch("browser", "slide")),
    vscode.commands.registerCommand("tinymist.previewDev", launchDevPreview),
    vscode.commands.registerCommand(
      "typst-preview.revealDocument",
      isCompat ? revealDocumentCompat : revealDocumentLsp,
    ),
    vscode.commands.registerCommand(
      "typst-preview.sync",
      isCompat ? panelSyncScrollCompat : panelSyncScrollLsp,
    ),
    vscode.commands.registerCommand("tinymist.doInspectPreviewState", () => {
      const tasks = Array.from(activeTask.values()).map((t) => {
        return {
          panel: !!t.panel,
          taskId: t.taskId,
        };
      });
      return {
        tasks,
      };
    }),
    vscode.commands.registerCommand("tinymist.doDisposePreview", ({ taskId }) => {
      for (const t of activeTask.values()) {
        if (t.taskId === taskId) {
          t.panel?.dispose();
          return;
        }
      }
    }),
  );

  // Additional routines for the compat mode.
  if (isCompat) {
    previewPostActivateCompat(context);
  }

  launchImpl = isCompat ? launchPreviewCompat : launchPreviewLsp;

  /**
   * Options to launch the preview.
   *
   * @param isBrowsing Whether to launch the preview in browsing mode. It switches the previewing
   * document on focus change.
   * @param isDev Whether to launch the preview in development mode. It fixes some random arguments
   * to help the `vite dev` server connect the language server via WebSocket.
   */
  interface LaunchOpts {
    isBrowsing?: boolean;
    isDev?: boolean;
    // isDev = false
  }

  /**
   * Gets current active editor and launches the preview.
   *
   * @param kind Which kind of preview to launch, either in external browser or in builtin vscode
   * webview.
   * @param mode The preview mode, either viewing as a document or as a slide.
   */
  function launch(kind: "browser" | "webview", mode: "doc" | "slide", opts?: LaunchOpts) {
    return async () => {
      activeEditor = activeEditor || vscode.window.activeTextEditor;
      if (!activeEditor) {
        vscode.window.showWarningMessage("No active editor");
        return;
      }
      const bindDocument = activeEditor.document;
      return launchImpl({
        kind,
        context,
        editor: activeEditor,
        bindDocument,
        mode,
        isBrowsing: opts?.isBrowsing || false,
        isDev: opts?.isDev || false,
      }).catch((e) => {
        vscode.window.showErrorMessage(`failed to launch preview: ${e}`);
      });
    };
  }
}

export function previewDeactivate() {
  previewDeactivateCompat();
}

function getPreviewConfCompat<T>(s: string) {
  const conf = vscode.workspace.getConfiguration();
  const t = conf.get<T>(`tinymist.preview.${s}`);
  const tAuto = conf.inspect<T>(`tinymist.preview.${s}`);
  const t2 = conf.get<T>(`typst-preview.${s}`);
  if (t === tAuto?.defaultValue && t2 !== undefined) {
    return t2;
  }

  return t;
}

/**
 * The arguments for launching the preview in a builtin vscode webview.
 */
interface OpenPreviewInWebViewArgs {
  /**
   * The extension context.
   */
  context: vscode.ExtensionContext;
  /**
   * The preview task arguments.
   */
  task: LaunchInWebViewTask;
  /**
   * The active editor owning *typst language document*.
   */
  activeEditor: vscode.TextEditor;
  /**
   * The port number of the data plane server.
   *
   * The server is already opened by the {@link launchImpl} function.
   */
  dataPlanePort: string | number;
  /**
   * The existing webview panel to reuse.
   */
  webviewPanel?: vscode.WebviewPanel;
  /**
   * Additional cleanup routine when the webview panel is disposed.
   */
  panelDispose: () => void;
}

/**
 * Launches the preview in a builtin vscode webview.
 *
 * @param See {@link OpenPreviewInWebViewArgs}.
 * @returns
 */
export async function openPreviewInWebView({
  context,
  task,
  activeEditor,
  dataPlanePort,
  webviewPanel,
  panelDispose,
}: OpenPreviewInWebViewArgs) {
  const basename = path.basename(activeEditor.document.fileName);
  const fontendPath = path.resolve(context.extensionPath, "out/frontend");
  // Create and show a new WebView
  const panel =
    webviewPanel !== undefined
      ? webviewPanel
      : vscode.window.createWebviewPanel(
          "typst-preview",
          `${basename} (Preview)`,
          getTargetViewColumn(activeEditor.viewColumn),
          {
            enableScripts: true,
            retainContextWhenHidden: true,
          },
        );

  // todo: bind Document.onDidDispose, but we did not find a similar way.
  panel.onDidDispose(async () => {
    panelDispose();
    console.log("killed preview services");
  });

  // Determines arguments for the preview HTML.
  const previewMode = task.mode === "doc" ? "Doc" : "Slide";
  const previewState: PersistPreviewState = {
    mode: task.mode,
    isNotPrimary: !!task.isNotPrimary,
    isBrowsing: !!task.isBrowsing,
    uri: activeEditor.document.uri.toString(),
  };
  const previewStateEncoded = Buffer.from(JSON.stringify(previewState), "utf-8").toString("base64");

  // Substitutes arguments in the HTML content.
  let html = await getPreviewHtml(context);
  // todo: not needed anymore, but we should test it and remove it later.
  html = html.replace(
    /\/typst-webview-assets/g,
    `${panel.webview.asWebviewUri(vscode.Uri.file(fontendPath)).toString()}/typst-webview-assets`,
  );
  html = html.replace("preview-arg:previewMode:Doc", `preview-arg:previewMode:${previewMode}`);
  html = html.replace("preview-arg:state:", `preview-arg:state:${previewStateEncoded}`);
  html = html.replace(
    "ws://127.0.0.1:23625",
    translateExternalURL(`ws://127.0.0.1:${dataPlanePort}`),
  );

  // Sets the HTML content to the webview panel.
  // This will reload the webview panel if it's already opened.
  panel.webview.html = html;

  // Forwards the localhost port to the external URL. Since WebSocket runs over HTTP, it should be fine.
  // https://code.visualstudio.com/api/advanced-topics/remote-extensions#forwarding-localhost
  await vscode.env.asExternalUri(
    vscode.Uri.parse(translateExternalURL(`http://127.0.0.1:${dataPlanePort}`)),
  );
  return panel;
}

/**
 * Holds the task control block for each preview task. This is used for editor interactions like
 * bidirectional jumps between source panels and preview panels.
 */
interface TaskControlBlock {
  /// related panel
  panel?: vscode.WebviewPanel;
  /// random task id
  taskId: string;
}
const activeTask = new Map<vscode.TextDocument, TaskControlBlock>();

async function launchPreviewLsp(task: LaunchInBrowserTask | LaunchInWebViewTask) {
  const { kind, context, editor, bindDocument, webviewPanel, isBrowsing, isDev, isNotPrimary } =
    task;

  /**
   * Can only open one preview for one document.
   */
  if (activeTask.has(bindDocument)) {
    const { panel } = activeTask.get(bindDocument)!;
    if (panel) {
      panel.reveal();
    }
    return { message: "existed" };
  }

  const taskId = Math.random().toString(36).substring(7);
  const filePath = bindDocument.uri.fsPath;

  const refreshStyle = getPreviewConfCompat<string>("refresh") || "onSave";
  const scrollSyncMode =
    ScrollSyncModeEnum[getPreviewConfCompat<ScrollSyncMode>("scrollSync") || "never"];
  const enableCursor = getPreviewConfCompat<boolean>("cursorIndicator") || false;

  const disposes = new DisposeList();
  registerPreviewTaskDispose(taskId, disposes);

  const { dataPlanePort, staticServerPort, isPrimary } = await invokeLspCommand();
  if (!dataPlanePort || !staticServerPort) {
    disposes.dispose();
    throw new Error(`Failed to launch preview ${filePath}`);
  }

  // update real primary state
  task.isNotPrimary = !isPrimary;

  if (isPrimary) {
    let connectUrl = translateExternalURL(`ws://127.0.0.1:${dataPlanePort}`);
    contentPreviewProvider.then((p) => p.postActivate(connectUrl));
    disposes.add(() => {
      contentPreviewProvider.then((p) => p.postDeactivate(connectUrl));
    });
  }

  let panel: vscode.WebviewPanel | undefined = undefined;
  switch (kind) {
    case "webview": {
      panel = await openPreviewInWebView({
        context,
        task,
        activeEditor: editor,
        dataPlanePort,
        webviewPanel,
        panelDispose() {
          disposes.dispose();
          tinymist.killPreview(taskId);
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
  });
  return { message: "ok", taskId };

  async function invokeLspCommand() {
    console.log(`Preview Command ${filePath}`);
    const partialRenderingArgs = getPreviewConfCompat<boolean>("partialRendering")
      ? ["--partial-rendering"]
      : [];
    const ivArgs = getPreviewConfCompat("invertColors");
    const invertColorsArgs = ivArgs ? ["--invert-colors", JSON.stringify(ivArgs)] : [];
    const previewInSlideModeArgs = task.mode === "slide" ? ["--preview-mode=slide"] : [];
    const dataPlaneHostArgs = !isDev ? ["--data-plane-host", "127.0.0.1:0"] : [];

    const previewArgs = [
      "--task-id",
      taskId,
      "--refresh-style",
      refreshStyle,
      ...dataPlaneHostArgs,
      ...partialRenderingArgs,
      ...invertColorsArgs,
      ...previewInSlideModeArgs,
      ...(isNotPrimary ? ["--not-primary"] : []),
      filePath,
    ];

    const { dataPlanePort, staticServerPort, isPrimary } = await (isBrowsing
      ? tinymist.startBrowsingPreview(previewArgs)
      : tinymist.startPreview(previewArgs));
    console.log(
      `Launched preview, browsing:${isBrowsing}, data plane port:${dataPlanePort}, static server port:${staticServerPort}`,
    );

    if (enableCursor) {
      reportPosition(editor, "changeCursorPosition");
    }

    if (scrollSyncMode !== ScrollSyncModeEnum.never) {
      // See comment of reportPosition function to get context about multi-file project related logic.
      const src2docHandler = (e: vscode.TextEditorSelectionChangeEvent) => {
        const editor = e.textEditor;
        const kind = e.kind;

        // console.log(
        //     `selection changed, kind: ${kind && vscode.TextEditorSelectionChangeKind[kind]}`
        // );
        const shouldScrollPanel =
          // scroll by mouse
          kind === vscode.TextEditorSelectionChangeKind.Mouse ||
          // scroll by keyboard typing
          (scrollSyncMode === ScrollSyncModeEnum.onSelectionChange &&
            kind === vscode.TextEditorSelectionChangeKind.Keyboard);
        if (shouldScrollPanel) {
          // console.log(`selection changed, sending src2doc jump request`);
          reportPosition(editor, "panelScrollTo");
        }

        if (enableCursor) {
          reportPosition(editor, "changeCursorPosition");
        }
      };

      disposes.add(vscode.window.onDidChangeTextEditorSelection(src2docHandler, 500));
    }

    return { staticServerPort, dataPlanePort, isPrimary };
  }

  async function reportPosition(
    editorToReport: vscode.TextEditor,
    event: "changeCursorPosition" | "panelScrollTo",
  ) {
    const scrollRequest: PanelScrollOrCursorMoveRequest = {
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

  const taskId = args?.taskId;
  for (const t of activeTask.values()) {
    if (taskId && t.taskId !== taskId) {
      continue;
    }

    const scrollRequest: PanelScrollOrCursorMoveRequest = {
      event: "panelScrollTo",
      filepath: activeEditor.document.uri.fsPath,
      line: activeEditor.selection.active.line,
      character: activeEditor.selection.active.character,
    };
    scrollPreviewPanel(t.taskId, scrollRequest);
  }
}

async function scrollPreviewPanel(taskId: string, scrollRequest: ScrollPreviewRequest) {
  if ("filepath" in scrollRequest) {
    const filepath = scrollRequest.filepath;
    if (filepath.includes("extension-output")) {
      console.log("skip extension-output file", filepath);
      return;
    }
  }

  tinymist.scrollPreview(taskId, scrollRequest);
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
    private readonly htmlContent: string,
  ) {}

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken,
  ) {
    this._view = webviewView; // 将已经准备好的 HTML 设置为 Webview 内容

    const fontendPath = path.resolve(this.context.extensionPath, "out/frontend");
    let html = this.htmlContent.replace(
      /\/typst-webview-assets/g,
      `${this._view.webview
        .asWebviewUri(vscode.Uri.file(fontendPath))
        .toString()}/typst-webview-assets`,
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
    // console.log("postOutlineItemProvider", outline);
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
            : vscode.TreeItemCollapsibleState.None,
        );
      }),
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
    },
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

interface PersistPreviewState {
  mode: "doc" | "slide";
  isNotPrimary: boolean;
  isBrowsing: boolean;
  uri: string;
}

class TypstPreviewSerializer implements vscode.WebviewPanelSerializer<PersistPreviewState> {
  context: vscode.ExtensionContext;

  constructor(context: vscode.ExtensionContext) {
    this.context = context;
  }

  async deserializeWebviewPanel(webviewPanel: vscode.WebviewPanel, state: PersistPreviewState) {
    // console.log("deserializeWebviewPanel", state);
    if (!state) {
      return;
    }

    const uri = vscode.Uri.parse(state.uri);
    // toString again to get the canonical form
    const uriStr = uri.toString();

    // open this file and show in editor
    const doc =
      vscode.workspace.textDocuments.find((doc) => {
        return doc.uri.toString() === uriStr;
      }) || (await vscode.workspace.openTextDocument(uri));
    const editor = await vscode.window.showTextDocument(doc, getSensibleTextEditorColumn(), true);

    const bindDocument = editor.document;
    const mode = state.mode;
    const isNotPrimary = state.isNotPrimary;
    const isBrowsing = state.isBrowsing;

    await launchImpl({
      kind: "webview",
      context: this.context,
      editor,
      bindDocument,
      mode,
      webviewPanel,
      isBrowsing,
      isNotPrimary,
    });
  }
}
