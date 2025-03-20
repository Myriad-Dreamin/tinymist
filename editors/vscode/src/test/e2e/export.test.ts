// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import type { Context } from ".";
import * as fs from "fs";
import { hash } from "crypto";

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("export");
  await workspaceCtx.suite("export", async (suite) => {
    // const uri = workspaceCtx.workspaceUri();
    const uri = workspaceCtx.getWorkspace("export");
    vscode.window.showInformationMessage("Start export tests.");

    console.log("Start all tests on ", uri.fsPath);

    suite.addTest("export by test", async () => {
      const mainUrl = vscode.Uri.joinPath(uri, "main.typ");

      const mainTyp = await ctx.openDocument(mainUrl);

      // check and clear target directory
      const targetDir = vscode.Uri.joinPath(uri, "target");
      if (fs.existsSync(targetDir.fsPath)) {
        fs.rmdirSync(targetDir.fsPath, { recursive: true });
      }

      const exported: string[] = [];
      await vscode.commands.executeCommand<string>("tinymist.exportCurrentPdf");
      for (const kind of ["Pdf", "Png", "Svg", "Html", "Markdown", "Text"] as const) {
        const outputPath = await vscode.commands.executeCommand<string>("tinymist.export", kind);
        exported.push(outputPath);
      }

      const dirPat = /target[\\/]typst[\\/]/;
      for (const path of exported) {
        if (!path) {
          throw new Error(`Failed to export ${exported}`);
        }

        ctx.expect(!!dirPat.exec(path), `${path} is not under correct directory`).eq(true);
        ctx.expect(fs.existsSync(path), `${path} does not exist`).eq(true);

        if (path.endsWith(".png")) {
          const sha256 = hash("sha256", fs.readFileSync(path), "hex");
          ctx
            .expect(sha256, `sha256:${sha256}`)
            .eq("4523673a2ab4ce07de888830e3a84c2e70529703d904ac38138cab904a15dca8");
        }

        if (path.endsWith(".txt")) {
          const content = fs.readFileSync(path, "utf-8");
          ctx
            .expect(content)
            .eq(`A Hello World Example of Export Typst Document to Various FormatsHello World.`);
        }
      }

      // close the editor
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    });
  });
}
