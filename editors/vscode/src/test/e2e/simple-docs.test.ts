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

    const completionLabel = (item: vscode.CompletionItem) => {
      if (typeof item.label === "string") {
        return item.label;
      }
      return item.label.label;
    };

    suite.addTest("starts Client", async () => {
      const mainTyp = await ctx.openDocument(
        vscode.Uri.joinPath(workspaceUri, "completion-base.typ")
      );
      const pong = await ctx.completion<vscode.CompletionList>(
        mainTyp.document.uri,
        new vscode.Position(7, 2)
      );
      ctx.expect(pong.items.map(completionLabel)).to.include.members(["aa", "aab", "aabc"]);

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });

    suite.addTest("starts Preview", async () => {
      await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "preview-skyzh-cv.typ"));
      let resp = (await vscode.commands.executeCommand("typst-preview.preview")) as any;
      ctx.expect(resp).to.have.property("taskId");
      const { taskId } = resp;

      let previewState: any = await vscode.commands.executeCommand(
        "tinymist.doInspectPreviewState"
      );
      ctx.expect(previewState.tasks).to.have.lengthOf(1);
      ctx.expect(previewState.tasks[0].taskId).to.be.equal(taskId);
      ctx.expect(previewState.tasks[0].panel).to.be.true;

      await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "preview-hello-world.typ"));
      resp = await vscode.commands.executeCommand("typst-preview.preview");
      ctx.expect(resp).to.have.property("taskId");
      const { taskId: taskId2 } = resp;

      previewState = await vscode.commands.executeCommand("tinymist.doInspectPreviewState");
      ctx.expect(previewState.tasks).to.have.lengthOf(2);

      await ctx.openDocument(vscode.Uri.joinPath(workspaceUri, "preview-skyzh-cv.typ"));
      resp = await vscode.commands.executeCommand("typst-preview.preview");
      ctx.expect(resp.message).to.be.equal("existed");

      await vscode.commands.executeCommand("tinymist.doDisposePreview", { taskId });
      await vscode.commands.executeCommand("tinymist.doDisposePreview", { taskId: taskId2 });

      previewState = await vscode.commands.executeCommand("tinymist.doInspectPreviewState");
      ctx.expect(previewState.tasks).to.have.lengthOf(0);

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });

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
        // We add non-atomical edit to test lagged diagnostics
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
  });
}
