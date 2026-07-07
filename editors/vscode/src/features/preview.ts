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
  ejectPreviewPanelCompat,
} from "./preview-compat";
import {
  PanelScrollOrCursorMoveRequest,
  registerPreviewTaskDispose,
  ScrollPreviewRequest,
  tinymist,
} from "../lsp";
import { l10nMsg } from "../l10n";
import { IContext } from "../context";
import { extensionState } from "../state";
import {
  invalidatePreviewerCache,
  preloadPreviewer,
  resolvePreviewer,
  setPreviewBuiltinSourceMode,
  type PreviewerSourceMetadata,
  type ResolvedPreviewer,
} from "./previewer";
import { loadStoredViewerWindowState } from "./preview-window-state";

/**
 * The launch preview implementation which depends on `isCompat` of previewActivate.
 */
let launchImpl: typeof launchPreviewLsp;
const previewerErrorReported = Symbol("tinymistPreviewerErrorReported");

type ReportedPreviewerError = Error & {
  [previewerErrorReported]?: true;
};

function hasReportedPreviewerError(error: unknown): boolean {
  return (
    error instanceof Error && (error as ReportedPreviewerError)[previewerErrorReported] === true
  );
}

function reportPreviewerError(error: unknown, action: "load" | "open"): Error {
  const cause = error instanceof Error ? error.message : String(error);
  const message =
    action === "open"
      ? `Could not open Typst preview because the configured previewer could not be loaded: ${cause}`
      : `Could not load the configured Typst previewer: ${cause}`;
  console.error(message);
  void vscode.window.showErrorMessage(message);

  const reported = error instanceof Error ? error : new Error(message);
  (reported as ReportedPreviewerError)[previewerErrorReported] = true;
  return reported;
}

/**
 * The state corresponding to the focusing preview panel.
 */
export interface PreviewPanelContext {
  panel: vscode.WebviewPanel;
  state: PersistPreviewState;
}

/**
 * Preload the preview resources to reduce the latency of the first preview.
 * @param context The extension context.
 */
export function previewPreload(context: vscode.ExtensionContext) {
  void preloadPreviewer(context).catch((error) => {
    if (!hasReportedPreviewerError(error)) {
      reportPreviewerError(error, "load");
    }
  });
}

/**
 * Activate the typst preview feature. This is the "the main entry" of the preview feature.
 *
 * @param context The extension context.
 * @param isCompat Whether the preview feature is activated in the old `mgt19937.typst-preview`
 * extension.
 */
export function previewActivate(context: vscode.ExtensionContext, isCompat: boolean) {
  setPreviewBuiltinSourceMode(isCompat ? "compat" : "tinymist");

  // Provides `ContentView` (ContentPreviewProvider) at the sidebar, which is a list of thumbnail
  // images.
  const provider = new ContentPreviewProvider(context);
  resolveContentPreviewProvider(provider);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(
      isCompat ? "typst-preview.content-preview" : "tinymist.preview.content-preview",
      provider,
    ),
  );
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

  const launchBrowsingPreview = launch("webview", "doc", { isBrowsing: true });
  const launchDevPreview = (mode: "doc" | "slide") => launch("webview", mode, { isDev: true });
  // Registers preview commands, check `package.json` for descriptions.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (
        !event.affectsConfiguration("tinymist.previewer") &&
        !event.affectsConfiguration("tinymist.exportTarget")
      ) {
        return;
      }
      invalidatePreviewerCache();
      void provider.reloadIfVisible();
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(() => {
      invalidatePreviewerCache();
      void provider.reloadIfVisible();
    }),
    vscode.commands.registerCommand("tinymist.browsingPreview", launchBrowsingPreview),
    vscode.commands.registerCommand("typst-preview.preview", launch("webview", "doc")),
    vscode.commands.registerCommand("typst-preview.browser", launch("browser", "doc")),
    vscode.commands.registerCommand("typst-preview.preview-slide", launch("webview", "slide")),
    vscode.commands.registerCommand("typst-preview.browser-slide", launch("browser", "slide")),
    vscode.commands.registerCommand("tinymist.previewDev", launchDevPreview("doc")),
    vscode.commands.registerCommand("tinymist.previewDevSlide", launchDevPreview("slide")),
    ...(isCompat
      ? [
          vscode.commands.registerCommand("typst-preview.eject", ejectPreviewPanelCompat),
          vscode.commands.registerCommand("typst-preview.revealDocument", revealDocumentCompat),
          vscode.commands.registerCommand("typst-preview.sync", panelSyncScrollCompat),
        ]
      : [
          vscode.commands.registerCommand("typst-preview.eject", ejectPreviewPanelLsp),
          vscode.commands.registerCommand("typst-preview.revealDocument", revealDocumentLsp),
          vscode.commands.registerCommand("typst-preview.sync", panelSyncScrollLsp),
        ]),
    vscode.commands.registerCommand("tinymist.doInspectPreviewState", () => {
      const tasks = Array.from(activeTask.values()).map((t) => {
        return {
          panel: !!t.panel,
          taskId: t.taskId,
          source: t.previewSource,
        };
      });
      return {
        tasks,
      };
    }),
    vscode.commands.registerCommand("tinymist.doDisposePreview", ({ taskId }) => {
      for (const t of activeTask.values()) {
        if (t.taskId === taskId) {
          if (t.panel) {
            t.panel.dispose();
          } else {
            t.dispose?.();
            void tinymist.killPreview(t.taskId);
          }
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
    isNotPrimary?: boolean;
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
      const activeEditor = IContext.currentActiveEditor() || vscode.window.activeTextEditor;
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
        isNotPrimary: opts?.isNotPrimary || false,
      }).catch((e) => {
        if (hasReportedPreviewerError(e)) {
          return;
        }
        vscode.window.showErrorMessage(`failed to launch preview: ${e}`);
      });
    };
  }

  async function launchForURI(
    uri: vscode.Uri,
    kind: "browser" | "webview",
    mode: "doc" | "slide",
    opts?: LaunchOpts,
  ) {
    const doc =
      vscode.workspace.textDocuments.find((doc) => {
        return doc.uri.toString() === uri.toString();
      }) || (await vscode.workspace.openTextDocument(uri));
    const editor = await vscode.window.showTextDocument(doc, getSensibleTextEditorColumn(), true);

    const bindDocument = editor.document;
    const isBrowsing = opts?.isBrowsing;
    const isDev = opts?.isDev;
    const isNotPrimary = opts?.isNotPrimary;

    await launchImpl({
      kind,
      context,
      editor,
      bindDocument,
      mode,
      isBrowsing,
      isDev,
      isNotPrimary,
    });
  }

  /**
   * Ejects the preview panel to the external browser.
   */
  async function ejectPreviewPanelLsp() {
    const focusingContext = extensionState.getFocusingPreviewPanelContext();
    if (!focusingContext) {
      vscode.window.showWarningMessage("No active preview panel");
      return;
    }
    const { panel, state } = focusingContext;

    // Close the preview panel, basically kill the previous preview task.
    panel.dispose();

    await launchForURI(vscode.Uri.parse(state.uri), "browser", state.mode, {
      isBrowsing: state.isBrowsing,
      isDev: state.isDev,
      isNotPrimary: state.isNotPrimary,
    });
  }
}

export function previewDeactivate() {
  invalidatePreviewerCache();
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
  panelDispose: () => Promise<void>;
  /**
   * Resolved previewer, if the caller already needed it to decide how to launch.
   */
  previewer?: ResolvedPreviewer;
}

export interface OpenedPreviewWebview {
  panel: vscode.WebviewPanel;
  previewSource: PreviewerSourceMetadata;
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
  previewer: resolvedPreviewer,
}: OpenPreviewInWebViewArgs): Promise<OpenedPreviewWebview> {
  const basename = path.basename(activeEditor.document.fileName);
  let previewer = resolvedPreviewer;
  try {
    previewer ??= await resolvePreviewer(context);
  } catch (error) {
    throw reportPreviewerError(error, "open");
  }
  if (previewer.handlePreview) {
    throw reportPreviewerError(
      new Error("the configured previewer handles document preview without a webview"),
      "open",
    );
  }
  // Create and show a new WebView
  const panel =
    webviewPanel !== undefined
      ? webviewPanel
      : vscode.window.createWebviewPanel(
          "typst-preview",
          `${basename}${l10nMsg(" (Preview)")}`,
          getTargetViewColumn(activeEditor.viewColumn),
          {
            enableScripts: true,
            localResourceRoots: previewer.localResourceRoots,
            retainContextWhenHidden: true,
          },
        );
  panel.webview.options = {
    enableScripts: true,
    localResourceRoots: previewer.localResourceRoots,
  };

  const previewState: PersistPreviewState = {
    mode: task.mode,
    isNotPrimary: !!task.isNotPrimary,
    isBrowsing: !!task.isBrowsing,
    isDev: !!task.isDev,
    uri: activeEditor.document.uri.toString(),
  };

  const updateActivePanel = () => {
    if (panel.active) {
      extensionState.mut.focusingPreviewPanelContext = {
        panel,
        state: previewState,
      };
    }
  };

  // NOTE: To avoid missing the auto revealing of webview initialization.
  updateActivePanel();
  panel.onDidChangeViewState(updateActivePanel);

  // todo: bind Document.onDidDispose, but we did not find a similar way.
  panel.onDidDispose(async () => {
    if (extensionState.getFocusingPreviewPanelContext()?.panel === panel) {
      extensionState.mut.focusingPreviewPanelContext = undefined;
    }
    await panelDispose();
    console.log("killed preview services");
  });

  // Determines arguments for the preview HTML.
  const previewMode = task.mode === "doc" ? "Doc" : "Slide";
  const previewStateEncoded = Buffer.from(JSON.stringify(previewState), "utf-8").toString("base64");

  // Substitutes arguments in the HTML content.
  let html = rewritePreviewAssetRoot(previewer, panel.webview);
  html = html.replace("preview-arg:previewMode:Doc", `preview-arg:previewMode:${previewMode}`);
  html = html.replace("preview-arg:state:", `preview-arg:state:${previewStateEncoded}`);
  // Forwards the localhost port to the external URL. Since WebSocket runs over HTTP, it should be fine.
  // https://code.visualstudio.com/api/advanced-topics/remote-extensions#forwarding-localhost
  html = html.replace("ws://127.0.0.1:23625", await externalDataPlaneHost(dataPlanePort));

  // Sets the HTML content to the webview panel.
  // This will reload the webview panel if it's already opened.
  panel.webview.html = html;
  return {
    panel,
    previewSource: previewer.source,
  };
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
  /// previewer metadata
  previewSource?: PreviewerSourceMetadata;
  /// cleanup routine for previews that do not own a webview panel
  dispose?: () => void;
}
const activeTask = new Map<vscode.TextDocument, TaskControlBlock>();

async function launchPreviewLsp(task: LaunchInBrowserTask | LaunchInWebViewTask) {
  const { kind, context, editor, bindDocument, webviewPanel, isBrowsing, isDev, isNotPrimary } =
    task;

  /**
   * Can only open one preview for one document.
   */
  const existingTask = activeTask.get(bindDocument);
  if (existingTask) {
    const { panel } = existingTask;
    if (panel) {
      panel.reveal();
      return { message: "existed" };
    }

    existingTask.dispose?.();
    await tinymist.killPreview(existingTask.taskId);
  }

  const taskId = Math.random().toString(36).substring(7);
  const filePath = bindDocument.uri.fsPath;

  const disposes = new DisposeList();
  registerPreviewTaskDispose(taskId, disposes);

  const { dataPlanePort, staticServerPort, isPrimary } = await invokeLspCommand();
  if (!dataPlanePort || !staticServerPort) {
    disposes.dispose();
    throw new Error(`Failed to launch preview ${filePath}`);
  }

  // update real primary state
  task.isNotPrimary = !isPrimary;

  let resolvedPreviewer: ResolvedPreviewer | undefined = undefined;
  if (kind === "webview") {
    try {
      resolvedPreviewer = await resolvePreviewer(context);
    } catch (error) {
      disposes.dispose();
      throw reportPreviewerError(error, "open");
    }
  }
  const isHandledByPreviewer = kind === "webview" && !!resolvedPreviewer?.handlePreview;

  if (isPrimary && !isHandledByPreviewer) {
    const connectUrl = translateExternalURL(`ws://127.0.0.1:${dataPlanePort}`);
    contentPreviewProvider.then((p) => p.postActivate(connectUrl));
    disposes.add(() => {
      contentPreviewProvider.then((p) => p.postDeactivate(connectUrl));
    });
  }

  let panel: vscode.WebviewPanel | undefined = undefined;
  let previewSource: PreviewerSourceMetadata | undefined = undefined;
  switch (kind) {
    case "webview": {
      if (resolvedPreviewer?.handlePreview) {
        try {
          const dataPlaneHost = resolvedPreviewer.preferExternalDataPlaneHost
            ? await externalDataPlaneHost(dataPlanePort)
            : localDataPlaneHost(dataPlanePort);
          const previewHandle = await resolvedPreviewer.handlePreview({
            taskId,
            documentUri: bindDocument.uri.toString(),
            documentPath: filePath,
            mode: task.mode,
            target: resolvedPreviewer.source.target ?? "paged",
            dataPlaneHost,
            dataPlanePort,
            staticServerPort,
            initialWindowState: loadStoredViewerWindowState(context),
            isBrowsing: !!isBrowsing,
            isPrimary: !!isPrimary,
          });
          addPreviewHandleDispose(disposes, previewHandle);
        } catch (error) {
          disposes.dispose();
          await tinymist.killPreview(taskId);
          throw error;
        }
        previewSource = resolvedPreviewer.source;
        break;
      }

      const openedPreview = await openPreviewInWebView({
        context,
        task,
        activeEditor: editor,
        dataPlanePort,
        webviewPanel,
        previewer: resolvedPreviewer,
        async panelDispose() {
          disposes.dispose();
          await tinymist.killPreview(taskId);
        },
      });
      panel = openedPreview.panel;
      previewSource = openedPreview.previewSource;
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
    previewSource,
    dispose: () => disposes.dispose(),
  });
  disposes.add(() => {
    if (activeTask.get(bindDocument)?.taskId === taskId) {
      activeTask.delete(bindDocument);
    }
  });
  return { message: "ok", taskId };

  async function invokeLspCommand() {
    let prevSelection: EditorSelection | undefined = undefined;
    const scrollSyncMode =
      ScrollSyncModeEnum[getPreviewConfCompat<ScrollSyncMode>("scrollSync") || "never"];
    const enableCursor = getPreviewConfCompat<boolean>("cursorIndicator") || false;

    console.log(`Preview Command ${filePath}`);
    const previewInSlideModeArgs = task.mode === "slide" ? ["--preview-mode=slide"] : [];
    const dataPlaneHostArgs = !isDev ? ["--data-plane-host", "127.0.0.1:0"] : [];

    const previewArgs = [
      "--task-id",
      taskId,
      ...dataPlaneHostArgs,
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
      const src2docHandler = (e: vscode.TextEditorSelectionChangeEvent) => {
        const editor = e.textEditor;
        const kind = e.kind;

        const shouldScrollPanel =
          // scroll by mouse
          kind === vscode.TextEditorSelectionChangeKind.Mouse ||
          // scroll by keyboard typing
          (scrollSyncMode === ScrollSyncModeEnum.onSelectionChange &&
            kind === vscode.TextEditorSelectionChangeKind.Keyboard);
        if (shouldScrollPanel) {
          // console.log(`selection changed, sending src2doc jump request`);
          mayReportPosition(editor, "panelScrollTo");
        }

        if (enableCursor) {
          reportPosition(editor, "changeCursorPosition");
        }
      };

      disposes.add(vscode.window.onDidChangeTextEditorSelection(src2docHandler, 500));
    }

    return { staticServerPort, dataPlanePort, isPrimary };

    /**
     * Reports the position of the editor when necessary.
     */
    function mayReportPosition(editor: vscode.TextEditor, event: "panelScrollTo") {
      // For multiple selections, we don't try to scroll the preview panel.
      if (editor.selections.length > 1) {
        return;
      }
      // For adjacent selections, we don't try to scroll the preview panel.
      if (adjacentSelection(editor, prevSelection)) {
        return;
      }
      // Updates selection and reports the position.
      prevSelection = {
        uri: editor.document.uri,
        start: editor.selection.start,
        end: editor.selection.end,
      };
      return reportPosition(editor, event);
    }
  }

  interface EditorSelection {
    uri: vscode.Uri;
    start: vscode.Position;
    end: vscode.Position;
  }

  function adjacentSelection(editor: vscode.TextEditor, prevSelection?: EditorSelection): boolean {
    // If there is no previous position, we cannot determine if the current position is adjacent.
    // Or if the previous position is not from the same document, we cannot determine either.
    // It is intended to compare uri equality by reference, not by value.
    if (!prevSelection || prevSelection.uri !== editor.document.uri) {
      return false;
    }

    // Any of the current selection start or end shares the same position with the previous
    // selection start or end, we consider it as adjacent.
    const currentStart = editor.selection.start;
    const currentEnd = editor.selection.end;
    return (
      currentStart.isEqual(prevSelection.start) ||
      currentEnd.isEqual(prevSelection.end) ||
      currentStart.isEqual(prevSelection.end) ||
      currentEnd.isEqual(prevSelection.start)
    );
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

function localDataPlaneHost(dataPlanePort: string | number): string {
  return `ws://127.0.0.1:${dataPlanePort}`;
}

async function externalDataPlaneHost(dataPlanePort: string | number): Promise<string> {
  const uri = await vscode.env.asExternalUri(vscode.Uri.parse(`http://127.0.0.1:${dataPlanePort}`));
  return uri.toString().replace(/^http/, "ws");
}

function addPreviewHandleDispose(disposes: DisposeList, previewHandle: unknown) {
  if (!previewHandle) {
    return;
  }

  if (typeof previewHandle === "function") {
    disposes.add(() => previewHandle());
    return;
  }

  const disposable = previewHandle as vscode.Disposable;
  if (typeof disposable.dispose === "function") {
    disposes.add(disposable);
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
export const contentPreviewProvider = new Promise<ContentPreviewProvider>((resolve) => {
  resolveContentPreviewProvider = resolve;
});

let resolveOutlineProvider: (value: OutlineProvider) => void = () => {};
export const outlineProvider = new Promise<OutlineProvider>((resolve) => {
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

  constructor(private readonly context: vscode.ExtensionContext) {}

  public async resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken,
  ) {
    this._view = webviewView;

    webviewView.webview.onDidReceiveMessage((data) => {
      switch (data.type) {
        case "started": {
          // on content preview restarted
          this.resetHost();
          break;
        }
      }
    });

    await this.reloadIfVisible();
  }

  async reloadIfVisible() {
    if (!this._view) {
      return;
    }

    const previewer = await resolvePreviewer(this.context);
    const html = rewritePreviewAssetRoot(previewer, this._view.webview).replace(
      "ws://127.0.0.1:23625",
      ``,
    );

    this._view.webview.options = {
      enableScripts: true,
      localResourceRoots: previewer.localResourceRoots,
    };
    this._view.webview.html = html;
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

function rewritePreviewAssetRoot(previewer: ResolvedPreviewer, webview: vscode.Webview): string {
  const resourceRoot =
    previewer.localResourceRoots[0] ?? vscode.Uri.file(path.dirname(previewer.htmlUri.fsPath));
  const webviewAssetRoot = `${webview.asWebviewUri(resourceRoot).toString()}/typst-webview-assets`;
  return previewer.html.replace(/(?:\.\/|\/)typst-webview-assets/g, webviewAssetRoot);
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
    const detachedHint = span ? `` : `, detached`;

    const pos = this.data.position;

    const label = this.label
      ? typeof this.label === "string"
        ? this.label
        : this.label?.label
      : "<no-label>";

    if (pos) {
      this.tooltip = `${label} in page ${pos.page_no}, at (${pos.x.toFixed(3)} pt, ${pos.y.toFixed(3)} pt)${detachedHint}`;
      this.description = `page: ${pos.page_no}, at (${pos.x.toFixed(1)} pt, ${pos.y.toFixed(1)} pt)${detachedHint}`;
    } else {
      this.tooltip = `${label}${detachedHint}`;
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
  isDev: boolean;
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
    const isDev = state.isDev;

    await launchImpl({
      kind: "webview",
      context: this.context,
      editor,
      bindDocument,
      mode,
      webviewPanel,
      isBrowsing,
      isDev,
      isNotPrimary,
    });
  }
}
