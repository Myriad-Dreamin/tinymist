import * as vscode from "vscode";
import * as path from "path";
import { readFile } from "fs/promises";

async function loadHTMLFile(context: vscode.ExtensionContext, relativePath: string) {
    const filePath = path.resolve(context.extensionPath, relativePath);
    const fileContents = await readFile(filePath, "utf8");
    return fileContents;
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

    panel.webview.onDidReceiveMessage(async (message) => {
        console.log("onDidReceiveMessage", message);
        switch (message.type) {
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
                break;
            }
            default: {
                console.error("Unknown message type", message.type);
                break;
            }
        }
        panel.dispose();
    });

    panel.onDidDispose(async () => {});

    let html = await loadHTMLFile(context, "./out/editor-tools/index.html");
    panel.webview.html = html;
}
