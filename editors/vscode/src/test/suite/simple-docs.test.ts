// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import type { Context } from ".";

export async function getTests(ctx: Context) {
    await ctx.openWorkspace("simple-docs");
    await ctx.suite("starts Client", (suite) => {
        vscode.window.showInformationMessage("Start all tests.");
        const workspaceUri = ctx.workspaceUri();
        console.log("Start all tests on ", workspaceUri.fsPath);

        suite.addTest("starts Client", async () => {
            const mainTyp = await ctx.openDocument(
                vscode.Uri.joinPath(workspaceUri, "completion-base.typ")
            );
            const pong = await ctx.completion<vscode.CompletionList>(
                mainTyp.document.uri,
                new vscode.Position(7, 2)
            );
            ctx.expect(pong.items.map((e) => e.label)).to.include.members(["aa", "aab", "aabc"]);

            // close the editor
            await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
        });
    });
}
