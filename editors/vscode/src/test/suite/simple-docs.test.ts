// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import type { Context } from ".";

export async function getTests(ctx: Context) {
    // await ctx.openWorkspace("simple-docs");
    await ctx.suite("starts Client", (suite) => {
        vscode.window.showInformationMessage("Start all tests.");
        // const workspaceUri = ctx.workspaceUri();
        const workspaceUri = ctx.getWorkspace("simple-docs");
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

        const hasDiag = (
            change: [vscode.DiagnosticChangeEvent, [vscode.Uri, vscode.Diagnostic[]][]],
            cnt: number
        ) => {
            // flatten the array with setting uri
            const diagnostics = change[1]
                .map((e) => {
                    for (const diag of e[1]) {
                        (diag as any).uri = e[0];
                    }
                    return e[1];
                })
                .flat();
            ctx.expect(diagnostics.length).to.be.equal(
                cnt,
                `Expected ${cnt} diagnostics, got ${JSON.stringify(diagnostics, undefined, 1)}`
            );
        };

        suite.addTest("diagnostics works well", async () => {
            const mainUrl = vscode.Uri.joinPath(workspaceUri, "diagnostics.typ");
            let diagnostics;

            // create some definite error in the file
            diagnostics = await ctx.diagnostics(async () => {
                const mainTyp = await ctx.openDocument(mainUrl);
                // replace the content of the file
                return await mainTyp.edit((edit) => {
                    edit.replace(new vscode.Range(0, 0, 0, 0), `#`);
                });
            });
            hasDiag(diagnostics, 1);

            // change focus
            diagnostics = await ctx.diagnostics(async () => {
                await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "diagnostics2.typ"));
            });
            hasDiag(diagnostics, 0);

            // switch back to the first file
            diagnostics = await ctx.diagnostics(async () => {
                await ctx.openDocument(mainUrl);
            });
            hasDiag(diagnostics, 1);

            // clear content
            diagnostics = await ctx.diagnostics(async () => {
                const mainTyp = await ctx.openDocument(mainUrl);
                // replace the content of the file
                return await mainTyp.edit((edit) => {
                    edit.delete(new vscode.Range(0, 0, 0, 1));
                });
            });
            hasDiag(diagnostics, 0);

            // close the editor
            await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
        });
    });
}
