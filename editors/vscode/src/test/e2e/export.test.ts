// You can import and use all API from the 'vscode' module
// as well as import your extension to test it

import { hash } from "node:crypto";
import * as fs from "node:fs";
import * as vscode from "vscode";
import type { ExportOpts } from "../../cmd.export";
import type { ExportKind } from "../../features/export";
import type { ExportResponse } from "../../lsp";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("export");
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
    const dirPat = /target[\\/]typst[\\/]/;

    const prepareMain = async (mainPath: string) => {
      const mainUrl = vscode.Uri.joinPath(baseUri, mainPath);

      await ctx.openDocument(mainUrl);
    };

    const exportDoc = async (mainPath: string, kind: ExportKind, opts?: ExportOpts) => {
      await prepareMain(mainPath);

      return await vscode.commands.executeCommand<ExportResponse | null>(
        "tinymist.export",
        kind,
        opts,
      );
    };

    const getFileHash = (path?: string | null) => {
      if (!path || !fs.existsSync(path)) {
        throw new Error(`File ${path} does not exist`);
      }
      ctx.expect(!!dirPat.exec(path), `${path} is not under correct directory`).eq(true);
      return hash("sha256", fs.readFileSync(path), "hex");
    };

    const expectSingleHash = (response: ExportResponse | null) => {
      if (!response) {
        throw new Error("No response from export command");
      }
      if ("items" in response) {
        throw new Error("Expected single export, got multiple");
      }
      const sha256 = getFileHash(response.path);
      return ctx.expect(sha256.slice(0, 8), `sha256:${sha256}`);
    };

    const expectPaged = (response: ExportResponse | null) => {
      if (!response) {
        throw new Error("No response from export command");
      }
      if (!("items" in response)) {
        throw new Error("Expected multi-page export, got single");
      }

      return ctx.expect(
        response.items.map((item) => ({
          page: item.page,
          hash: getFileHash(item.path).slice(0, 8),
        })),
      );
    };

    suite.addTest("export current pdf", async () => {
      await prepareMain("main.typ");

      const resp = await vscode.commands.executeCommand<ExportResponse | null>(
        "tinymist.exportCurrentPdf",
      );
      expectSingleHash(resp).to.be.a("string");
    });

    suite.addTest("export pdf", async () => {
      const resp = await exportDoc("main.typ", "Pdf", { creationTimestamp: "0" });
      expectSingleHash(resp).eq("f5d1a181");
    });

    suite.addTest("export html", async () => {
      const resp = await exportDoc("main.typ", "Html");
      expectSingleHash(resp).eq("a55cf03e");
    });

    suite.addTest("export markdown", async () => {
      const resp = await exportDoc("main.typ", "Markdown");
      expectSingleHash(resp).eq("62ca0c72");
    });

    suite.addTest("export tex", async () => {
      const resp = await exportDoc("main.typ", "TeX");
      expectSingleHash(resp).eq("492c3e62");
    });

    suite.addTest("export text", async () => {
      const resp = await exportDoc("main.typ", "Text");
      expectSingleHash(resp).eq("8ae8f637");
    });

    suite.addTest("export query", async () => {
      const resp = await exportDoc("main.typ", "Query", { format: "json", selector: "heading" });
      expectSingleHash(resp).eq("a08f208d");
    });

    /* suite.addTest("export png", async () => {
      const resp = await exportDoc("main.typ", "Png");
      expectSingleHash(resp).eq("8ae8f637");

      // expectPaged(resp).eq([{ page: 1, hash: "4523673a" }]);
    });*/
  });
}
