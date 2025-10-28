// You can import and use all API from the 'vscode' module
// as well as import your extension to test it

import { hash } from "node:crypto";
import * as fs from "node:fs";
import * as vscode from "vscode";
import type { ExportActionOpts, ExportOpts } from "../../cmd.export";
import type { ExportKind } from "../../features/export";
import type { ExportResponse } from "../../lsp";
import { base64DecodeToBytes } from "../../util";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  const dirPat = /target[\\/]typst[\\/]/;

  const getFileHash = (path?: string | null) => {
    if (!path || !fs.existsSync(path)) {
      throw new Error(`File ${path} does not exist`);
    }
    ctx.expect(!!dirPat.exec(path), `${path} is not under correct directory`).eq(true);
    return hash("sha256", fs.readFileSync(path), "hex");
  };

  const expectSingleHash = (response: ExportResponse | null, ignoreHash: boolean = false) => {
    if (!response) {
      throw new Error("No response from export command");
    }
    if ("items" in response) {
      throw new Error("Expected single export, got multiple");
    }
    console.log(response.path);
    const sha256 = getFileHash(response.path);
    return ctx.expect(ignoreHash ? undefined : sha256.slice(0, 8), `sha256:${sha256}`);
  };

  const expectPaged = (response: ExportResponse | null, ignoreHash: boolean = false) => {
    if (!response) {
      throw new Error("No response from export command");
    }
    if (!("items" in response)) {
      throw new Error("Expected multi-page export, got single");
    }

    const expected = response.items.map((item) => ({
      page: item.page,
      hash: ignoreHash ? undefined : getFileHash(item.path).slice(0, 8),
    }));

    return ctx.expect(expected, `sha256:${expected.map((e) => e.hash).join(",")}`).to.deep;
  };

  const expectPagedData = (response: ExportResponse | null) => {
    if (!response) {
      throw new Error("No response from export command");
    }
    if (!("items" in response)) {
      throw new Error("Expected multi-page export, got single");
    }

    for (const item of response.items) {
      ctx.expect(item.data).to.be.a("string");
    }

    const expected = response.items.map((item) => ({
      page: item.page,
      hash: hash("sha256", base64DecodeToBytes(item.data as string), "hex").slice(0, 8),
    }));

    return ctx.expect(expected, `sha256:${expected.map((e) => e.hash).join(",")}`).to.deep;
  };

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

    const prepareMain = async (mainPath: string) => {
      const mainUrl = vscode.Uri.joinPath(baseUri, mainPath);

      await ctx.openDocument(mainUrl);
    };

    const exportDoc = async (
      mainPath: string,
      kind: ExportKind,
      opts?: ExportOpts,
      actionOpts?: ExportActionOpts,
    ) => {
      await prepareMain(mainPath);

      return await vscode.commands.executeCommand<ExportResponse | null>(
        "tinymist.export",
        kind,
        opts,
        actionOpts,
      );
    };

    // NOTE: For svg tests, the output (especially glyph id) may vary between different environments. So we do not check hash.

    suite.addTest("export current pdf", async () => {
      await prepareMain("main.typ");

      const resp = await vscode.commands.executeCommand<ExportResponse | null>(
        "tinymist.exportCurrentPdf",
      );
      expectSingleHash(resp).to.be.a("string");
    });

    suite.addTest("export pdf", async () => {
      const resp = await exportDoc("main.typ", "Pdf", { creationTimestamp: "0" });
      expectSingleHash(resp).eq("86c3e3b3");
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

    suite.addTest("export png", async () => {
      const resp = await exportDoc("main.typ", "Png");
      expectPaged(resp).eq([{ page: 0, hash: "a3987ce8" }]);
    });

    suite.addTest("export svg", async () => {
      const resp = await exportDoc("main.typ", "Svg");
      expectPaged(resp, true).eq([{ page: 0, hash: undefined }]);
    });

    suite.addTest("export png paged all", async () => {
      const resp = await exportDoc("paged.typ", "Png", { pageNumberTemplate: "paged-{p}" });
      expectPaged(resp).eq([
        { page: 0, hash: "27d34da8" },
        { page: 1, hash: "a97c7cc8" },
        { page: 2, hash: "08dfb2df" },
      ]);
    });

    suite.addTest("export png paged partial", async () => {
      const resp = await exportDoc("paged.typ", "Png", {
        pages: ["1"],
        pageNumberTemplate: "paged-partial-{p}",
      });
      expectPaged(resp).eq([{ page: 0, hash: "27d34da8" }]);
    });

    suite.addTest("export png paged merged", async () => {
      const resp = await exportDoc("paged.typ", "Png", {
        pages: ["2-3"],
        merge: {},
      });
      expectSingleHash(resp).eq("9b87f1ce");
    });

    suite.addTest("export svg paged all", async () => {
      const resp = await exportDoc("paged.typ", "Svg", { pageNumberTemplate: "paged-{p}" });
      expectPaged(resp, true).eq([
        { page: 0, hash: undefined },
        { page: 1, hash: undefined },
        { page: 2, hash: undefined },
      ]);
    });

    suite.addTest("export svg paged partial", async () => {
      const resp = await exportDoc("paged.typ", "Svg", {
        pages: ["2"],
        pageNumberTemplate: "paged-partial-{p}",
      });
      expectPaged(resp, true).eq([{ page: 1, hash: undefined }]);
    });

    suite.addTest("export svg paged merged", async () => {
      const resp = await exportDoc("paged.typ", "Svg", {
        pages: ["1-2"],
        merge: {},
      });
      expectSingleHash(resp, true).eq(undefined);
    });

    suite.addTest("export png paged all no-write", async () => {
      const resp = await exportDoc(
        "paged.typ",
        "Png",
        { pageNumberTemplate: "paged-{p}" },
        { write: false },
      );
      expectPagedData(resp).eq([
        { page: 0, hash: "27d34da8" },
        { page: 1, hash: "a97c7cc8" },
        { page: 2, hash: "08dfb2df" },
      ]); // this should be same as "export png paged all"
    });
  });
}
