import {
    type ExtensionContext,
    workspace,
    window,
    commands,
    ViewColumn,
    Uri,
    WorkspaceConfiguration,
    TextEditor,
    ExtensionMode,
} from "vscode";
import * as vscode from "vscode";
import * as path from "path";
import * as child_process from "child_process";

import {
    LanguageClient,
    type LanguageClientOptions,
    type ServerOptions,
} from "vscode-languageclient/node";
import vscodeVariables from "vscode-variables";

let client: LanguageClient | undefined = undefined;

export function activate(context: ExtensionContext): Promise<void> {
    return startClient(context).catch((e) => {
        void window.showErrorMessage(`Failed to activate tinymist: ${e}`);
        throw e;
    });
}

async function startClient(context: ExtensionContext): Promise<void> {
    const config = workspace.getConfiguration("tinymist");
    const serverCommand = getServer(config);
    const fontPaths = vscode.workspace.getConfiguration().get<string[]>("tinymist.fontPaths");
    const noSystemFonts =
        vscode.workspace.getConfiguration().get<boolean | null>("tinymist.noSystemFonts") === true;
    const run = {
        command: serverCommand,
        args: [
            ...["--mode", "server"],
            /// The `--mirror` flag is only used in development/test mode for testing
            ...(context.extensionMode != ExtensionMode.Production
                ? ["--mirror", "tinymist-lsp.log"]
                : []),
            ...(fontPaths ?? []).flatMap((fontPath) => ["--font-path", vscodeVariables(fontPath)]),
            ...(noSystemFonts ? ["--no-system-fonts"] : []),
        ],
        options: { env: Object.assign({}, process.env, { RUST_BACKTRACE: "1" }) },
    };
    console.log("use arguments", run);
    const serverOptions: ServerOptions = {
        run,
        debug: run,
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "typst" }],
        initializationOptions: config,
    };

    client = new LanguageClient(
        "tinymist",
        "Tinymist Typst Language Server",
        serverOptions,
        clientOptions
    );

    window.onDidChangeActiveTextEditor((editor: TextEditor | undefined) => {
        if (editor?.document.languageId !== "typst") {
            return commandActivateDoc(undefined);
        }
        return commandActivateDoc(editor);
    });

    context.subscriptions.push(
        commands.registerCommand("tinymist.exportCurrentPdf", commandExportCurrentPdf)
    );
    context.subscriptions.push(
        commands.registerCommand("typst-lsp.pinMainToCurrent", () => commandPinMain(true))
    );
    context.subscriptions.push(
        commands.registerCommand("typst-lsp.unpinMain", () => commandPinMain(false))
    );
    context.subscriptions.push(commands.registerCommand("tinymist.showPdf", commandShowPdf));
    context.subscriptions.push(commands.registerCommand("tinymist.clearCache", commandClearCache));
    context.subscriptions.push(
        commands.registerCommand("tinymist.runCodeLens", commandRunCodeLens)
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.initTemplate", commandInitTemplate)
    );

    return client.start();
}

export function deactivate(): Promise<void> | undefined {
    return client?.stop();
}

function getServer(conf: WorkspaceConfiguration): string {
    const pathInConfig = conf.get<string | null>("serverPath");
    if (pathInConfig !== undefined && pathInConfig !== null && pathInConfig !== "") {
        const validation = validateServer(pathInConfig);
        if (!validation.valid) {
            throw new Error(
                `\`tinymist.serverPath\` (${pathInConfig}) does not point to a valid tinymist binary:\n${validation.message}`
            );
        }
        return pathInConfig;
    }
    const windows = process.platform === "win32";
    const suffix = windows ? ".exe" : "";
    const binaryName = "tinymist" + suffix;

    const bundledPath = path.resolve(__dirname, binaryName);

    const bundledValidation = validateServer(bundledPath);
    if (bundledValidation.valid) {
        return bundledPath;
    }

    const binaryValidation = validateServer(binaryName);
    if (binaryValidation.valid) {
        return binaryName;
    }

    throw new Error(
        `Could not find a valid tinymist binary.\nBundled: ${bundledValidation.message}\nIn PATH: ${binaryValidation.message}`
    );
}

function validateServer(path: string): { valid: true } | { valid: false; message: string } {
    try {
        console.log("validate", path, "args", ["--mode", "probe"]);
        const result = child_process.spawnSync(path, ["--mode", "probe"]);
        if (result.status === 0) {
            return { valid: true };
        } else {
            const statusMessage = result.status !== null ? [`return status: ${result.status}`] : [];
            const errorMessage =
                result.error?.message !== undefined ? [`error: ${result.error.message}`] : [];
            const messages = [statusMessage, errorMessage];
            const messageSuffix =
                messages.length !== 0 ? `:\n\t${messages.flat().join("\n\t")}` : "";
            const message = `Failed to launch '${path}'${messageSuffix}`;
            return { valid: false, message };
        }
    } catch (e) {
        if (e instanceof Error) {
            return { valid: false, message: `Failed to launch '${path}': ${e.message}` };
        } else {
            return { valid: false, message: `Failed to launch '${path}': ${JSON.stringify(e)}` };
        }
    }
}

async function commandExportCurrentPdf(): Promise<string | undefined> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    const uri = activeEditor.document.uri.toString();

    const res = await client?.sendRequest<string | null>("workspace/executeCommand", {
        command: "tinymist.exportPdf",
        arguments: [uri],
    });
    console.log("export pdf", res);
    if (res === null) {
        return undefined;
    }
    return res;
}

/**
 * Implements the functionality for the 'Show PDF' button shown in the editor title
 * if a `.typ` file is opened.
 */
async function commandShowPdf(): Promise<void> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    // only create pdf if it does not exist yet
    const pdfPath = await commandExportCurrentPdf();

    if (pdfPath === undefined) {
        // show error message
        await window.showErrorMessage("Failed to create PDF");
        return;
    }

    const pdfUri = Uri.file(pdfPath);

    // here we can be sure that the pdf exists
    await commands.executeCommand("vscode.open", pdfUri, ViewColumn.Beside);
}

async function commandClearCache(): Promise<void> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    const uri = activeEditor.document.uri.toString();

    await client?.sendRequest("workspace/executeCommand", {
        command: "tinymist.doClearCache",
        arguments: [uri],
    });
}

async function commandPinMain(isPin: boolean): Promise<void> {
    if (!isPin) {
        await client?.sendRequest("workspace/executeCommand", {
            command: "tinymist.pinMain",
            arguments: ["detached"],
        });
        return;
    }

    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    await client?.sendRequest("workspace/executeCommand", {
        command: "tinymist.pinMain",
        arguments: [activeEditor.document.uri.fsPath],
    });
}

async function commandInitTemplate(...args: string[]): Promise<void> {
    const initArgs: string[] = [];
    if (args.length === 2) {
        initArgs.push(...args);
    } else if (args.length > 0) {
        await vscode.window.showErrorMessage("Invalid arguments for initTemplate");
        return;
    } else {
        const mode = await vscode.window.showInputBox({
            title: "template from url or package spec id",
            prompt: "git or package spec with an optional version, you can also enters entire command, such as `typst init @preview/touying:0.3.2`",
        });
        initArgs.push(mode ?? "");
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
    }

    const fsPath = initArgs[1];
    const uri = Uri.file(fsPath);

    await client?.sendRequest("workspace/executeCommand", {
        command: "tinymist.doInitTemplate",
        arguments: [...initArgs],
    });

    await commands.executeCommand("vscode.openFolder", uri);
}

async function commandActivateDoc(editor: TextEditor | undefined): Promise<void> {
    await client?.sendRequest("workspace/executeCommand", {
        command: "tinymist.focusMain",
        arguments: [editor?.document.uri.fsPath],
    });
}

async function commandRunCodeLens(...args: string[]): Promise<void> {
    console.log("run code lens", args);
    if (args.length === 0) {
        return;
    }

    switch (args[0]) {
        case "preview": {
            void vscode.commands.executeCommand(`typst-preview.preview`);
            break;
        }
        case "preview-in": {
            // prompt for enum (doc, slide) with default
            const mode = await vscode.window.showQuickPick(["doc", "slide"], {
                title: "Preview Mode",
            });
            const target = await vscode.window.showQuickPick(["tab", "browser"], {
                title: "Target to preview in",
            });

            const command =
                (target === "tab" ? "preview" : "browser") + (mode === "slide" ? "-slide" : "");

            void vscode.commands.executeCommand(`typst-preview.${command}`);
            break;
        }
        case "export-pdf": {
            await commandShowPdf();
            break;
        }
        case "export-as": {
            const fmt = await vscode.window.showQuickPick(["pdf"], {
                title: "Format to export as",
            });

            if (fmt === "pdf") {
                await commandShowPdf();
            }
            break;
        }
        default: {
            console.error("unknown code lens command", args[0]);
        }
    }
}
