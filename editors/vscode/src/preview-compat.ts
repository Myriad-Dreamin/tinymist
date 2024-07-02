// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import * as path from "path";
import { getServer } from "./extension";

/// kill the probe task after 60s
const PROBE_TIMEOUT = 60_000;
let isTinymist = false;
let tinymistServerConfig: string | undefined;
let guy = "$(typst-guy)";

export async function setIsTinymist(config: Record<string, any>) {
    isTinymist = true;
    tinymistServerConfig = config.server;
    guy = "$(sync)";
}

export async function getPreviewCliPath(extensionPath?: string): Promise<string> {
    if (tinymistServerConfig) {
        return getServer(tinymistServerConfig!);
    }

    const { sync: spawnSync } = require("cross-spawn");

    const state = getPreviewCliPath as unknown as any;
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

export function previewStatusBarItemProcess(
    event: "Compiling" | "CompileSuccess" | "CompileError"
) {
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
