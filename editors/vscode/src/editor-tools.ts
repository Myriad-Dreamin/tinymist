import * as vscode from "vscode";
import * as path from "path";
import { readFile } from "fs/promises";
import { getFocusingFile } from "./extension";

async function loadHTMLFile(context: vscode.ExtensionContext, relativePath: string) {
    const filePath = path.resolve(context.extensionPath, relativePath);
    const fileContents = await readFile(filePath, "utf8");
    return fileContents;
}

const USER_PACKAGE_VERSION = "0.0.1";

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

export function getUserPackageData(context: vscode.ExtensionContext) {
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

export async function activateEditorTool(
    context: vscode.ExtensionContext,
    tool: "template-gallery" | "tracing" | "summary" | "symbol-picker"
) {
    // Create and show a new WebView
    const title = {
        "template-gallery": "Template Gallery",
        "symbol-picker": "Symbol Picker",
        tracing: "Tracing",
        summary: "Summary",
    }[tool];
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
        }
    );

    await activateEditorToolAt(context, tool, panel);
}

export class SymbolViewProvider implements vscode.WebviewViewProvider {
    constructor(private context: vscode.ExtensionContext) {}

    public resolveWebviewView(
        webviewView: vscode.WebviewView,
        _context: vscode.WebviewViewResolveContext,
        _token: vscode.CancellationToken
    ) {
        webviewView.webview.options = {
            // Allow scripts in the webview
            enableScripts: true,
        };

        activateEditorToolAt(this.context, "symbol-picker", webviewView);
    }
}

async function activateEditorToolAt(
    context: vscode.ExtensionContext,
    tool: "template-gallery" | "tracing" | "summary" | "symbol-picker",
    panel: vscode.WebviewView | vscode.WebviewPanel
) {
    const dispose = () => {
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
                await vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(path));
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

    let html = await loadHTMLFile(context, "./out/editor-tools/index.html");
    // packageData

    html = html.replace(
        /`editor-tools-args:{"page": [^`]*?`/,
        `\`editor-tools-args:{"page": "${tool}"}\``
    );

    let afterReloadHtml = undefined;

    switch (tool) {
        case "template-gallery":
            const userPackageData = getUserPackageData(context);
            const packageData = JSON.stringify(userPackageData.data);
            html = html.replace(":[[preview:FavoritePlaceholder]]:", btoa(packageData));
            break;
        case "tracing": {
            const focusingFile = getFocusingFile();
            if (focusingFile === undefined) {
                await vscode.window.showErrorMessage("No focusing typst file");
                return;
            }
            const traceDataTask = vscode.commands.executeCommand(
                "tinymist.getDocumentTrace",
                focusingFile
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

            html = html.replace(":[[preview:DocumentMetrics]]:", btoa(docMetrics));
            html = html.replace(":[[preview:ServerInfo]]:", btoa(serverInfo));
            break;
        }
        case "symbol-picker": {
            // tinymist.getCurrentDocumentMetrics
            const result = await vscode.commands.executeCommand(
                "tinymist.getResources",
                "/symbols"
            );

            if (!result) {
                vscode.window.showErrorMessage("No resource");
                dispose();
                return;
            }

            const symbolInfo = JSON.stringify(result);
            html = html.replace(":[[preview:SymbolInformation]]:", btoa(symbolInfo));
            break;
        }
    }

    panel.webview.html = html;

    if (afterReloadHtml) {
        afterReloadHtml();
    }
}

const waitTimeList = [100, 200, 400, 1000, 1200, 1500, 1800, 2000];
async function fetchSummaryInfo(): Promise<[any | undefined, any | undefined]> {
    let res: [any | undefined, any | undefined] = [undefined, undefined];

    for (const to of waitTimeList) {
        const focusingFile = getFocusingFile();
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
                focusingFile
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
