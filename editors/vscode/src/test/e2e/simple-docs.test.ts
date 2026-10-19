// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  ctx.workspaceCtx("book");
  // await ctx.openWorkspace("simple-docs");
  await ctx.suite("starts Client", async (suite) => {
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
        vscode.Uri.joinPath(workspaceUri, "completion-base.typ"),
      );
      const pong = await ctx.completion<vscode.CompletionList>(
        mainTyp.document.uri,
        new vscode.Position(7, 2),
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
        "tinymist.doInspectPreviewState",
      );
      ctx.expect(previewState.tasks).to.have.lengthOf(1);
      ctx.expect(previewState.tasks[0].taskId).to.be.equal(taskId);
      ctx.expect(!!previewState.tasks[0].panel).to.be.equal(true);
      ctx.expect(previewState.tasks[0].source.kind).to.be.equal("builtin");

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

    const openPreviewDocument = async (name: string) => {
      const document = await vscode.workspace.openTextDocument(
        vscode.Uri.joinPath(workspaceUri, name),
      );
      ctx.expect(document.languageId).to.equal("typst");
      let focused = vscode.window.activeTextEditor?.document === document;
      const listener = vscode.window.onDidChangeActiveTextEditor((editor) => {
        focused = editor?.document === document;
      });
      try {
        await vscode.window.showTextDocument(document, {
          viewColumn: vscode.ViewColumn.One,
          preserveFocus: false,
          preview: false,
        });
        for (let i = 0; !focused && i < 40; i++) {
          await ctx.timeout(50);
        }
        ctx.expect(focused, `Expected active editor for ${name}`).to.equal(true);
      } finally {
        listener.dispose();
      }
      // focusMain is debounced by 100ms after the active-editor event.
      await ctx.timeout(200);
    };

    // Polls the panel title until it converges to the document actually being compiled by the
    // primary project, which the browsing preview renders.
    const expectTitle = async (taskId: string, expected: string) => {
      let title = "";
      for (let i = 0; i < 40; i++) {
        const previewState: any = await vscode.commands.executeCommand(
          "tinymist.doInspectPreviewState",
        );
        title = previewState.tasks.find(
          (task: { taskId: string }) => task.taskId === taskId,
        )?.title;
        if (title?.includes(expected)) {
          return;
        }
        await ctx.timeout(250);
      }
      ctx.expect(title).to.contain(expected);
    };

    suite.addTest("starts Browsing Preview", async () => {
      await openPreviewDocument("preview-pin-main.typ");
      const resp = await vscode.commands.executeCommand<{ taskId: string }>(
        "tinymist.browsingPreview",
      );
      ctx.expect(resp).to.have.property("taskId");
      const { taskId } = resp;

      try {
        let previewState: any = await vscode.commands.executeCommand(
          "tinymist.doInspectPreviewState",
        );
        ctx.expect(previewState.tasks).to.have.lengthOf(1);
        ctx.expect(previewState.tasks[0].taskId).to.be.equal(taskId);
        ctx.expect(!!previewState.tasks[0].panel).to.be.equal(true);
        await expectTitle(taskId, "preview-pin-main.typ");

        // The panel title follows the document being compiled in browsing mode.
        await openPreviewDocument("preview-pin-chapter.typ");
        await expectTitle(taskId, "preview-pin-chapter.typ");

        // The title follows the compiled entry instead of the focused document: after pinning
        // the main file, focusing the chapter document changes neither the rendered document nor
        // the title.
        await openPreviewDocument("preview-pin-main.typ");
        await vscode.commands.executeCommand("tinymist.pinMainToCurrent");
        await expectTitle(taskId, "preview-pin-main.typ");
        await openPreviewDocument("preview-pin-chapter.typ");
        await ctx.timeout(2000);
        previewState = await vscode.commands.executeCommand("tinymist.doInspectPreviewState");
        ctx.expect(previewState.tasks[0].title).to.contain("preview-pin-main.typ");

        // Unpinning makes the preview follow the focused document again.
        await vscode.commands.executeCommand("tinymist.unpinMain");
        await expectTitle(taskId, "preview-pin-chapter.typ");
      } finally {
        try {
          await vscode.commands.executeCommand("tinymist.unpinMain");
        } finally {
          await vscode.commands.executeCommand("tinymist.doDisposePreview", { taskId });
        }
      }

      const previewState: any = await vscode.commands.executeCommand(
        "tinymist.doInspectPreviewState",
      );
      ctx.expect(previewState.tasks).to.have.lengthOf(0);

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });

    suite.addTest("preview titles are task-scoped with compile status disabled", async () => {
      const config = vscode.workspace.getConfiguration("tinymist");
      const previous = config.inspect<string>("compileStatus")?.workspaceValue;
      const taskIds: string[] = [];
      try {
        await config.update("compileStatus", "disable", vscode.ConfigurationTarget.Workspace);
        // Restart so initialization definitely receives the disabled setting.
        await vscode.commands.executeCommand("tinymist.restartServer");
        await openPreviewDocument("preview-pin-main.typ");
        const browsing = await vscode.commands.executeCommand<{ taskId: string }>(
          "tinymist.browsingPreview",
        );
        ctx.expect(browsing).to.have.property("taskId");
        taskIds.push(browsing.taskId);

        await openPreviewDocument("preview-pin-chapter.typ");
        await expectTitle(browsing.taskId, "preview-pin-chapter.typ");
        // A second browsing request falls back to a dedicated compiler because the primary
        // already has a preview. Its title must follow that compiler, not all browsing panels.
        const dedicated = await vscode.commands.executeCommand<{ taskId: string }>(
          "tinymist.browsingPreview",
        );
        ctx.expect(dedicated).to.have.property("taskId");
        taskIds.push(dedicated.taskId);

        // Compiling a different primary entry must not rename the dedicated chapter panel.
        await openPreviewDocument("preview-pin-main.typ");
        await expectTitle(browsing.taskId, "preview-pin-main.typ");
        const state: any = await vscode.commands.executeCommand("tinymist.doInspectPreviewState");
        ctx
          .expect(
            state.tasks.find((task: { taskId: string }) => task.taskId === dedicated.taskId)?.title,
          )
          .to.contain("preview-pin-chapter.typ");
      } finally {
        try {
          await Promise.all(
            taskIds.map((taskId) =>
              vscode.commands.executeCommand("tinymist.doDisposePreview", { taskId }),
            ),
          );
        } finally {
          await config.update("compileStatus", previous, vscode.ConfigurationTarget.Workspace);
          await vscode.commands.executeCommand("tinymist.restartServer");
        }
      }
    });

    suite.addTest("restart server", async () => {
      const _mainTyp = await ctx.openDocument(
        vscode.Uri.joinPath(workspaceUri, "completion-base.typ"),
      );

      await vscode.commands.executeCommand("tinymist.restartServer");

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });
  });
}
