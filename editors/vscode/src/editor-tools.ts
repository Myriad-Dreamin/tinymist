import * as vscode from "vscode";
import * as path from "path";
import { readFile } from "fs/promises";

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

export async function activateEditorTool(context: vscode.ExtensionContext, tool: string) {
    if (
        tool !== "template-gallery" &&
        tool !== "tracing" &&
        tool !== "summary" &&
        tool !== "symbol-picker"
    ) {
        vscode.window.showErrorMessage(`Unknown editor tool: ${tool}`);
        return;
    }

    const title = {
        "template-gallery": "Template Gallery",
        "symbol-picker": "Symbol Picker",
        tracing: "Tracing",
        summary: "Summary",
    }[tool];

    // Create and show a new WebView
    const panel = vscode.window.createWebviewPanel(
        `tinymist-${tool}`,
        title,
        vscode.ViewColumn.Beside, // Which sides
        {
            enableScripts: true,
            retainContextWhenHidden: true,
        }
    );

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
                panel.dispose();
                break;
            }
            default: {
                console.error("Unknown message type", message.type);
                break;
            }
        }
    });

    panel.onDidDispose(async () => {});

    let html = await loadHTMLFile(context, "./out/editor-tools/index.html");
    // packageData

    html = html.replace(
        /`editor-tools-args:{"page": [^`]*?`/,
        `\`editor-tools-args:{"page": "${tool}"}\``
    );

    switch (tool) {
        case "template-gallery":
            const userPackageData = getUserPackageData(context);
            const packageData = JSON.stringify(userPackageData.data);
            html = html.replace(":[[preview:FavoritePlaceholder]]:", btoa(packageData));
            break;
        case "tracing":
            break;
        case "summary": {
            const [docMetrics, serverInfo] = await fetchSummaryInfo();

            if (!docMetrics || !serverInfo) {
                if (!docMetrics) {
                    vscode.window.showErrorMessage("No document metrics available");
                }
                if (!serverInfo) {
                    vscode.window.showErrorMessage("No server info");
                }

                panel.dispose();
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
                panel.dispose();
                return;
            }

            const symbolInfo = JSON.stringify(result);
            html = html.replace(":[[preview:SymbolInformation]]:", btoa(symbolInfo));
            break;
        }
    }

    panel.webview.html = html;
}

const waitTimeList = [100, 200, 400, 1000, 1200, 1500, 1800, 2000];
async function fetchSummaryInfo(): Promise<[any | undefined, any | undefined]> {
    let res: [any | undefined, any | undefined] = [undefined, undefined];

    for (const to of waitTimeList) {
        await work(res);
        if (res[0] && res[1]) {
            break;
        }
        // wait for a bit
        await new Promise((resolve) => setTimeout(resolve, to));
    }

    return res;

    async function work(res: [any | undefined, any | undefined]) {
        if (!res[0]) {
            const result = await vscode.commands.executeCommand(
                "tinymist.getCurrentDocumentMetrics"
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
