// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("book");

  await workspaceCtx.suite("browsing Preview with lockDatabase", async (suite) => {
    const workspaceUri = ctx.getWorkspace("lock-project");
    console.log("Start lockDatabase tests on ", workspaceUri.fsPath);

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

    suite.addTest("title follows the locked main file, not the focused chapter", async () => {
      const config = vscode.workspace.getConfiguration("tinymist");
      const previousResolution = config.inspect<string>("projectResolution")?.workspaceValue;
      const previousStatus = config.inspect<string>("compileStatus")?.workspaceValue;
      const tempRoot = fs.mkdtempSync(path.join(tmpdir(), "tinymist-lock-preview-"));
      let taskId: string | undefined;
      try {
        fs.cpSync(workspaceUri.fsPath, tempRoot, { recursive: true });
        // Start with no lock or cached routes so the test cannot pass using an earlier export.
        fs.rmSync(path.join(tempRoot, "tinymist.lock"), { force: true });
        const project = vscode.Uri.file(tempRoot);

        // Exercise both settings together: entry notifications must not depend on compile status.
        await config.update(
          "projectResolution",
          "lockDatabase",
          vscode.ConfigurationTarget.Workspace,
        );
        await config.update("compileStatus", "disable", vscode.ConfigurationTarget.Workspace);
        await vscode.commands.executeCommand("tinymist.restartServer");

        // Exporting the main file creates the lock and the path-material cache used to resolve
        // chapters. Wait for that asynchronous update before starting a new server.
        const main = vscode.Uri.joinPath(project, "main.typ");
        const lock = vscode.Uri.joinPath(project, "tinymist.lock");
        await ctx.openDocument(main);
        const exported = await vscode.commands.executeCommand<{ data: string }>(
          "tinymist.export",
          "Pdf",
          {},
          { write: false },
        );
        ctx.expect(exported?.data).to.be.a("string");
        let lockCreated = false;
        for (let i = 0; i < 40; i++) {
          try {
            const contents = Buffer.from(await vscode.workspace.fs.readFile(lock)).toString("utf8");
            if (contents.includes('main = "file:main.typ"') && contents.includes("[[route]]")) {
              lockCreated = true;
              break;
            }
          } catch {
            // The lock is not yet created.
          }
          await ctx.timeout(250);
        }
        ctx.expect(lockCreated, "Expected export to create tinymist.lock").to.equal(true);
        await vscode.commands.executeCommand("tinymist.restartServer");

        // Focus the chapter file. With lockDatabase, the server resolves it to main.typ.
        await ctx.openDocument(vscode.Uri.joinPath(project, "chapters", "chapter1.typ"));
        // Allow active-editor listeners and the focusMain debounce to settle.
        await ctx.timeout(200);

        const resp = await vscode.commands.executeCommand<{ taskId: string }>(
          "tinymist.browsingPreview",
        );
        ctx.expect(resp).to.have.property("taskId");
        taskId = resp.taskId;

        // The panel title must be the resolved main file, not the focused chapter.
        await expectTitle(taskId, "main.typ");
      } finally {
        try {
          if (taskId) {
            await vscode.commands.executeCommand("tinymist.doDisposePreview", { taskId });
          }
        } finally {
          // Restore configuration even when the assertion or preview cleanup fails.
          try {
            await config.update(
              "projectResolution",
              previousResolution,
              vscode.ConfigurationTarget.Workspace,
            );
            await config.update(
              "compileStatus",
              previousStatus,
              vscode.ConfigurationTarget.Workspace,
            );
            await vscode.commands.executeCommand("tinymist.restartServer");
          } finally {
            try {
              await vscode.commands.executeCommand("workbench.action.closeAllEditors");
            } finally {
              fs.rmSync(tempRoot, { recursive: true, force: true });
            }
          }
        }
      }
    });
  });
}
