// Inspired from https://github.com/camel-tooling/camel-lsp-client-vscode/blob/main/src/test/suite/completion.util.ts

import * as assert from "node:assert/strict";
import { readdir } from "fs/promises";
import * as path from "path";
import * as vscode from "vscode";
import * as fs from "fs";

class Test {
    readonly name: string;
    readonly promise: Promise<void>;

    constructor(name: string, promise: Promise<void>) {
        this.name = name;
        this.promise = promise;
    }
}

class Suite {
    tests: Test[];

    constructor() {
        this.tests = [];
    }

    public addTest(name: string, f: () => Promise<void>): void {
        const test = new Test(name, f());
        this.tests.push(test);
    }

    public async run(): Promise<void> {
        let failed = 0;
        for (const test of this.tests) {
            try {
                await test.promise;
                ok(`  ✔ ${test.name}`);
            } catch (e) {
                assert.ok(e instanceof Error);
                error(`  ✖︎ ${test.name}\n  ${e.stack}`);
                failed += 1;
            }
        }
        if (failed) {
            const plural = failed > 1 ? "s" : "";
            throw new Error(`${failed} failed test${plural}`);
        }
    }
}

type CompResponse = vscode.CompletionList | vscode.CompletionItem[];

export class Context {
    expect!: typeof import("chai").expect;

    public async suite(name: string, f: (ctx: Suite) => void): Promise<void> {
        const ctx = new Suite();
        f(ctx);
        try {
            ok(`⌛︎ ${name}`);
            await ctx.run();
            ok(`✔ ${name}`);
        } catch (e) {
            assert.ok(e instanceof Error);
            error(`✖︎ ${name}\n  ${e.stack}`);
            throw e;
        }
    }

    public async openWorkspace(workspace: string): Promise<void> {
        console.log(`Opening workspace ${workspace}`);

        const resolved = vscode.Uri.file(
            path.resolve(__dirname, "../../../e2e-workspaces/", workspace)
        );
        // assert directory exists
        assert.ok(fs.existsSync(resolved.fsPath), "Workspace directory does not exist");

        vscode.workspace.updateWorkspaceFolders(0, vscode.workspace.workspaceFolders?.length || 0, {
            uri: resolved,
        });
        assert(
            vscode.workspace.workspaceFolders?.length === 1,
            "Expected exactly one workspace folder"
        );
        assert.strictEqual(
            vscode.workspace.workspaceFolders[0].uri.toString(),
            resolved.toString(),
            "Unexpected workspace folder"
        );
    }

    workspaceUri(): vscode.Uri {
        const folders = vscode.workspace.workspaceFolders;
        assert.ok(folders, "No workspace folders");
        assert.strictEqual(folders.length, 1, "Expected exactly one workspace folder");
        return folders[0].uri;
    }

    async openDocument(docUri: vscode.Uri): Promise<vscode.TextEditor> {
        const doc = await vscode.workspace.openTextDocument(docUri);
        return await vscode.window.showTextDocument(doc);
    }

    async completion<T extends CompResponse = CompResponse>(
        docUri: vscode.Uri,
        position: vscode.Position
    ): Promise<T> {
        const doc = await vscode.workspace.openTextDocument(docUri);
        const editor = await vscode.window.showTextDocument(doc);
        editor.selection = new vscode.Selection(position, position);
        return await vscode.commands.executeCommand(
            "vscode.executeCompletionItemProvider",
            docUri,
            position
        );
    }
}

export async function run(): Promise<void> {
    const context = new Context();
    context.expect = (await importEsmModule<any>("chai")).expect;

    // exit process after timeout
    setTimeout(() => {
        console.error("Tests timed out");
        process.exit(81);
    }, 60000);

    const testFiles = (await readdir(path.resolve(__dirname))).filter((name) =>
        name.endsWith(".test.js")
    );
    for (const testFile of testFiles) {
        try {
            const testModule = require(path.resolve(__dirname, testFile));
            await testModule.getTests(context);
        } catch (e) {
            error(`${e}`);
            throw e;
        }
    }
}

function ok(message: string): void {
    // eslint-disable-next-line no-console
    console.log(`\x1b[32m${message}\x1b[0m`);
}

function error(message: string): void {
    // eslint-disable-next-line no-console
    console.error(`\x1b[31m${message}\x1b[0m`);
}

// https://stackoverflow.com/questions/65265420/how-to-prevent-typescript-from-transpiling-dynamic-imports-into-require
export async function importEsmModule<T>(name: string): Promise<T> {
    const module = eval(`(async () => {return await import("${name}")})()`);
    return module as T;
}
