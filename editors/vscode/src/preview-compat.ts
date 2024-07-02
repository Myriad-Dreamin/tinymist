// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from "vscode";
import * as path from "path";
import { spawn, sync as spawnSync } from "cross-spawn";

/// kill the probe task after 60s
const PROBE_TIMEOUT = 60_000;

export async function getPreviewCliPath(extensionPath?: string): Promise<string> {
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
