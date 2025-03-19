// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  await ctx.suite("diagnostics", (suite) => {
    vscode.window.showInformationMessage("Start all tests.");
    const workspaceUri = ctx.getWorkspace("diag");
    console.log("Start all tests on ", workspaceUri.fsPath);
    
    suite.addTest("diagnostics works well", async () => {
      const mainUrl = vscode.Uri.joinPath(workspaceUri, "diagnostics.typ");

      const largeDoc0 = "#for i in range(500) { lorem(i) };";
      const largeDoc = "#for i in range(500) { lorem(i) }; #test()";

      // create some definite error in the file
      await ctx.diagnostics(1, async () => {
        const mainTyp = await ctx.openDocument(mainUrl);
        // replace the content of the file with a large document
        await mainTyp.edit((edit) => {
          edit.replace(new vscode.Range(0, 0, 0, 0), largeDoc0);
        });
        await ctx.timeout(400);
        // We add non-atomic edit to test lagged diagnostics
        return await mainTyp.edit((edit) => {
          edit.replace(new vscode.Range(0, 0, 0, largeDoc0.length), largeDoc);
        });
      });
      // change focus
      await ctx.diagnostics(0, async () => {
        await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "diagnostics2.typ"));
      });
      // switch back to the first file
      await ctx.diagnostics(1, async () => {
        await ctx.openDocument(mainUrl);
      });
      // clear content
      await ctx.diagnostics(0, async () => {
        const mainTyp = await ctx.openDocument(mainUrl);
        // replace the content of the file
        return await mainTyp.edit((edit) => {
          edit.delete(new vscode.Range(0, 0, 0, largeDoc.length));
        });
      });

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });

    suite.addTest("typst0.13 diag hints", async () => {
      const mainUrl = vscode.Uri.joinPath(workspaceUri, "typst013.typ");

      const [_1, _2, diags] = await ctx.diagnostics(1, async () => {
        await ctx.openDocument(mainUrl);
        ctx.timeout(400);
      });

      ctx.expect(diags).to.have.lengthOf(1);
      const diag = diags[0];
      ctx.expect(diag.message).contains("Hint: Typst 0.13");
    });
  });
}
