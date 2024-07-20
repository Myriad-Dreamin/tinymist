import * as assert from "assert";

// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import * as lsp from "../../lsp";
import type { Context } from ".";

export async function getTests(ctx: Context) {
    await ctx.openWorkspace("simple-docs");
    await ctx.suite("starts Client", (suite) => {
        vscode.window.showInformationMessage("Start all tests.");
        const workspaceUri = ctx.workspaceUri();
        console.log("Start all tests on ", workspaceUri.fsPath);

        suite.addTest("starts Client", async () => {
            const mainTyp = await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "main.typ"));
            const client = await lsp.getClient();
            assert.ok(mainTyp && client, "Client is not started");
        });
    });
}
