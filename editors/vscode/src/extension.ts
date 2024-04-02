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
import { activateEditorTool, getUserPackageData } from "./editor-tools";
import { wordCountItemProcess } from "./ui-extends";

let client: LanguageClient | undefined = undefined;

export function activate(context: ExtensionContext): Promise<void> {
    return startClient(context).catch((e) => {
        void window.showErrorMessage(`Failed to activate tinymist: ${e}`);
        throw e;
    });
}

async function startClient(context: ExtensionContext): Promise<void> {
    let config: Record<string, any> = JSON.parse(
        JSON.stringify(workspace.getConfiguration("tinymist"))
    );

    {
        const keys = Object.keys(config);
        let values = keys.map((key) => config[key]);
        values = substVscodeVarsInConfig(keys, values);
        config = {};
        for (let i = 0; i < keys.length; i++) {
            config[keys[i]] = values[i];
        }
    }

    const serverCommand = getServer(config);
    const fontPaths = config.fontPaths as string[] | null;
    const withSystemFonts = (config.systemFonts as boolean | null) !== false;
    const run = {
        command: serverCommand,
        args: [
            ...["lsp"],
            /// The `--mirror` flag is only used in development/test mode for testing
            ...(context.extensionMode != ExtensionMode.Production
                ? ["--mirror", "tinymist-lsp.log"]
                : []),
            ...(fontPaths ?? []).flatMap((fontPath) => ["--font-path", vscodeVariables(fontPath)]),
            ...(withSystemFonts ? [] : ["--no-system-fonts"]),
        ],
        options: { env: Object.assign({}, process.env, { RUST_BACKTRACE: "1" }) },
    };
    console.log("use arguments", run);
    const serverOptions: ServerOptions = {
        run,
        debug: run,
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: "file", language: "typst" },
            { scheme: "untitled", language: "typst" },
        ],
        initializationOptions: config,
        middleware: {
            workspace: {
                async configuration(params, token, next) {
                    const items = params.items.map((item) => item.section);
                    const result = await next(params, token);
                    if (!Array.isArray(result)) {
                        return result;
                    }
                    return substVscodeVarsInConfig(items, result);
                },
            },
        },
    };

    client = new LanguageClient(
        "tinymist",
        "Tinymist Typst Language Server",
        serverOptions,
        clientOptions
    );

    client.onNotification("tinymist/compileStatus", (params) => {
        wordCountItemProcess(params);
    });

    window.onDidChangeActiveTextEditor((editor: TextEditor | undefined) => {
        if (editor?.document.isUntitled) {
            return;
        }
        const langId = editor?.document.languageId;
        // todo: plaintext detection
        // if (langId === "plaintext") {
        //     console.log("plaintext", langId, editor?.document.uri.fsPath);
        // }
        if (langId !== "typst") {
            return commandActivateDoc(undefined);
        }
        return commandActivateDoc(editor?.document.uri.fsPath);
    });
    vscode.workspace.onDidOpenTextDocument((doc: vscode.TextDocument) => {
        if (doc.isUntitled && window.activeTextEditor?.document === doc) {
            if (doc.languageId === "typst") {
                return commandActivateDoc("/untitled/" + doc.uri.fsPath);
            } else {
                return commandActivateDoc(undefined);
            }
        }
    });

    context.subscriptions.push(
        commands.registerCommand("tinymist.exportCurrentPdf", () => commandExport("Pdf"))
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.getCurrentDocumentMetrics", () =>
            commandGetCurrentDocumentMetrics()
        )
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.pinMainToCurrent", () => commandPinMain(true))
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.unpinMain", () => commandPinMain(false))
    );
    context.subscriptions.push(
        commands.registerCommand("typst-lsp.pinMainToCurrent", () => commandPinMain(true))
    );
    context.subscriptions.push(
        commands.registerCommand("typst-lsp.unpinMain", () => commandPinMain(false))
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.showPdf", () => commandShow("Pdf"))
    );
    context.subscriptions.push(commands.registerCommand("tinymist.clearCache", commandClearCache));
    context.subscriptions.push(
        commands.registerCommand("tinymist.runCodeLens", commandRunCodeLens)
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.initTemplate", (...args) =>
            commandInitTemplate(context, false, ...args)
        )
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.initTemplateInPlace", (...args) =>
            commandInitTemplate(context, true, ...args)
        )
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.showTemplateGallery", () =>
            commandShowTemplateGallery(context)
        )
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.showSummary", () => commandShowSummary(context))
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.showSymbolPicker", () =>
            commandShowSymbolPicker(context)
        )
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.traceCurrentFile", () => commandShowTrace(context))
    );
    context.subscriptions.push(
        commands.registerCommand("tinymist.showLog", () => {
            if (client) {
                client.outputChannel.show();
            }
        })
    );

    return client.start();
}

export function deactivate(): Promise<void> | undefined {
    return client?.stop();
}

function getServer(conf: Record<string, any>): string {
    const pathInConfig = conf.serverPath;
    if (pathInConfig) {
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
        console.log("validate", path, "args", ["probe"]);
        const result = child_process.spawnSync(path, ["probe"]);
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

async function commandExport(mode: string, extraOpts?: any): Promise<string | undefined> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    const uri = activeEditor.document.uri.fsPath;

    const res = await client?.sendRequest<string | null>("workspace/executeCommand", {
        command: `tinymist.export${mode}`,
        arguments: [uri, ...(extraOpts ? [extraOpts] : [])],
    });
    if (res === null) {
        return undefined;
    }
    return res;
}

async function commandGetCurrentDocumentMetrics(): Promise<any> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    const fsPath = activeEditor.document.uri.fsPath;

    const res = await client?.sendRequest<string | null>("workspace/executeCommand", {
        command: `tinymist.getDocumentMetrics`,
        arguments: [fsPath],
    });
    if (res === null) {
        return undefined;
    }
    return res;
}

/**
 * Implements the functionality for the 'Show PDF' button shown in the editor title
 * if a `.typ` file is opened.
 */
async function commandShow(kind: string, extraOpts?: any): Promise<void> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    // only create pdf if it does not exist yet
    const exportPath = await commandExport(kind, extraOpts);

    if (exportPath === undefined) {
        // show error message
        await window.showErrorMessage(`Failed to export ${kind}`);
        return;
    }

    const exportUri = Uri.file(exportPath);

    // find and replace exportUri
    // todo: we may find them in tabs
    vscode.window.tabGroups;

    let uriToFind = exportUri.toString();
    findTab: for (const editor of vscode.window.tabGroups.all) {
        for (const tab of editor.tabs) {
            if ((tab.input as any)?.uri?.toString() === uriToFind) {
                await vscode.window.tabGroups.close(tab, true);
                break findTab;
            }
        }
    }

    // here we can be sure that the pdf exists
    await commands.executeCommand("vscode.open", exportUri, {
        viewColumn: ViewColumn.Beside,
        preserveFocus: true,
    } as vscode.TextDocumentShowOptions);
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
            arguments: [null],
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

async function commandShowTemplateGallery(context: vscode.ExtensionContext): Promise<void> {
    await activateEditorTool(context, "template-gallery");
}

async function commandShowSummary(context: vscode.ExtensionContext): Promise<void> {
    await activateEditorTool(context, "summary");
}

async function commandShowSymbolPicker(context: vscode.ExtensionContext): Promise<void> {
    await activateEditorTool(context, "symbol-picker");
}

async function commandShowTrace(context: vscode.ExtensionContext): Promise<void> {
    const activeEditor = window.activeTextEditor;
    if (activeEditor === undefined) {
        return;
    }

    const uri = activeEditor.document.uri.toString();
    void uri;

    await activateEditorTool(context, "tracing");
}

async function commandInitTemplate(
    context: vscode.ExtensionContext,
    inPlace: boolean,
    ...args: string[]
): Promise<void> {
    const initArgs: string[] = [];
    if (!inPlace) {
        if (args.length === 2) {
            initArgs.push(...args);
        } else if (args.length > 0) {
            await vscode.window.showErrorMessage(
                "Invalid arguments for initTemplate, needs either all arguments or zero arguments"
            );
            return;
        } else {
            const mode = await getTemplateSpecifier();
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

        interface InitResult {
            entryPath: string;
        }

        const res: InitResult | undefined = await client?.sendRequest("workspace/executeCommand", {
            command: "tinymist.doInitTemplate",
            arguments: [...initArgs],
        });

        const workspaceRoot = workspace.workspaceFolders?.[0]?.uri.fsPath;
        if (res && workspaceRoot && uri.fsPath.startsWith(workspaceRoot)) {
            const entry = Uri.file(path.resolve(uri.fsPath, res.entryPath));
            await commands.executeCommand("vscode.open", entry, ViewColumn.Active);
        } else {
            // focus the new folder
            await commands.executeCommand("vscode.openFolder", uri);
        }
    } else {
        if (args.length === 1) {
            initArgs.push(...args);
        } else if (args.length > 0) {
            await vscode.window.showErrorMessage(
                "Invalid arguments for initTemplateInPlace, needs either all arguments or zero arguments"
            );
            return;
        } else {
            const mode = await getTemplateSpecifier();
            initArgs.push(mode ?? "");
        }

        const res: string | undefined = await client?.sendRequest("workspace/executeCommand", {
            command: "tinymist.doGetTemplateEntry",
            arguments: [...initArgs],
        });

        if (!res) {
            return;
        }

        const activeEditor = window.activeTextEditor;
        if (activeEditor === undefined) {
            return;
        }

        // insert content at the cursor
        activeEditor.edit((editBuilder) => {
            editBuilder.insert(activeEditor.selection.active, res);
        });
    }

    function getTemplateSpecifier(): Promise<string> {
        const data = getUserPackageData(context).data;
        const pkgSpecifiers: string[] = [];
        for (const ns of Object.keys(data)) {
            for (const pkgName of Object.keys(data[ns])) {
                const pkg = data[ns][pkgName];
                if (pkg?.isFavorite) {
                    pkgSpecifiers.push(`@${ns}/${pkgName}`);
                }
            }
        }

        return new Promise((resolve) => {
            const quickPick = window.createQuickPick();
            quickPick.placeholder =
                "git, package spec with an optional version, such as `@preview/touying:0.3.2`";
            quickPick.canSelectMany = false;
            quickPick.items = pkgSpecifiers.map((label) => ({ label }));
            quickPick.onDidAccept(() => {
                const selection = quickPick.activeItems[0];
                resolve(selection.label);
                quickPick.hide();
            });
            quickPick.onDidChangeValue(() => {
                // add a new code to the pick list as the first item
                if (!pkgSpecifiers.includes(quickPick.value)) {
                    const newItems = [quickPick.value, ...pkgSpecifiers].map((label) => ({
                        label,
                    }));
                    quickPick.items = newItems;
                }
            });
            quickPick.onDidHide(() => quickPick.dispose());
            quickPick.show();
        });
    }
}

async function commandActivateDoc(fsPath: string | undefined): Promise<void> {
    await client?.sendRequest("workspace/executeCommand", {
        command: "tinymist.focusMain",
        arguments: [fsPath],
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
            await commandShow("Pdf");
            break;
        }
        case "export-as": {
            enum FastKind {
                PDF = "PDF",
                SVG = "SVG (First Page)",
                SVGMerged = "SVG (Merged)",
                PNG = "PNG (First Page)",
                PNGMerged = "PNG (Merged)",
            }

            const fmt = await vscode.window.showQuickPick(
                [FastKind.PDF, FastKind.SVG, FastKind.SVGMerged, FastKind.PNG, FastKind.PNGMerged],
                {
                    title: "Format to export as",
                }
            );

            switch (fmt) {
                case FastKind.PDF:
                    await commandShow("Pdf");
                    break;
                case FastKind.SVG:
                    await commandShow("Svg");
                    break;
                case FastKind.SVGMerged:
                    await commandShow("Svg", { page: "merged" });
                    break;
                case FastKind.PNG:
                    await commandShow("Png");
                    break;
                case FastKind.PNGMerged:
                    await commandShow("Png", { page: "merged" });
                    break;
            }

            break;
        }
        default: {
            console.error("unknown code lens command", args[0]);
        }
    }
}

function substVscodeVars(str: string | null | undefined): string | undefined {
    if (str === undefined || str === null) {
        return undefined;
    }
    try {
        return vscodeVariables(str);
    } catch (e) {
        console.error("failed to substitute vscode variables", e);
        return str;
    }
}

const STR_VARIABLES = [
    "serverPath",
    "tinymist.serverPath",
    "rootPath",
    "tinymist.rootPath",
    "outputPath",
    "tinymist.outputPath",
];
const STR_ARR_VARIABLES = ["fontPaths", "tinymist.fontPaths"];

// todo: documentation that, typstExtraArgs won't get variable extended
function substVscodeVarsInConfig(keys: (string | undefined)[], values: unknown[]): unknown[] {
    return values.map((value, i) => {
        const k = keys[i];
        if (!k) {
            return value;
        }
        if (STR_VARIABLES.includes(k)) {
            return substVscodeVars(value as string);
        }
        if (STR_ARR_VARIABLES.includes(k)) {
            const paths = value as string[];
            if (!paths) {
                return undefined;
            }
            return paths.map((path) => substVscodeVars(path));
        }
        return value;
    });
}
