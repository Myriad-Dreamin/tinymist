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
    if (tool !== "template-gallery") {
        vscode.window.showErrorMessage(`Unknown editor tool: ${tool}`);
        return;
    }

    // Create and show a new WebView
    const panel = vscode.window.createWebviewPanel(
        "tinymist-editor-tool", // 标识符
        `Template Gallery`, // 面板标题
        vscode.ViewColumn.Beside, // 显示在编辑器的哪一侧
        {
            enableScripts: true, // 启用 JS
            retainContextWhenHidden: true,
        }
    );

    const userPackageData = getUserPackageData(context);
    const packageData = JSON.stringify(userPackageData.data);

    panel.webview.onDidReceiveMessage(async (message) => {
        console.log("onDidReceiveMessage", message);
        switch (message.type) {
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
    html = html.replace(":[[preview:FavoritePlaceholder]]:", btoa(packageData));
    panel.webview.html = html;
}
