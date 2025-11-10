// You can import and use all API from the 'vscode' module
// as well as import your extension to test it

import * as fs from "node:fs";
import * as vscode from "vscode";
import type { ExportResponse } from "../../lsp";
import { tinymist } from "../../lsp";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("export");

  const prepareMain = async (mainPath: string) => {
    const baseUri = workspaceCtx.getWorkspace("export");
    const mainUrl = vscode.Uri.joinPath(baseUri, mainPath);

    return await ctx.openDocument(mainUrl);
  };

  await workspaceCtx.suite("export", async (suite) => {
    // const uri = workspaceCtx.workspaceUri();
    const baseUri = workspaceCtx.getWorkspace("export");
    vscode.window.showInformationMessage("Start export tests.");

    console.log("Start all tests on ", baseUri.fsPath);

    // check and clear target directory
    const targetDir = vscode.Uri.joinPath(baseUri, "target");
    if (fs.existsSync(targetDir.fsPath)) {
      fs.rmdirSync(targetDir.fsPath, { recursive: true });
    }

    const editor = await prepareMain("main.typ");

    suite.addTest("eval paste function", async () => {
      const res = await tinymist.interactCodeContext(editor.document.uri.toString(), [
        {
          kind: "pathAt",
          code: "$root/x/y/z",
          inputs: {},
        },
      ]);
      ctx.expect(res).to.be.ok;
      if ("error" in res![0]) {
        ctx.expect.fail(res![0].error);
      } else {
        ctx.expect(res![0].value).to.be.eq("$root/x/y/z");
      }
    });
  });
}
