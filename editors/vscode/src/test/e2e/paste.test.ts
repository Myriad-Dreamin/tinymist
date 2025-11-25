// You can import and use all API from the 'vscode' module
// as well as import your extension to test it

import * as vscode from "vscode";
import * as path from "path";
import { getDesiredNewFilePath } from "../../features/drop-paste";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  const workspaceCtx = ctx.workspaceCtx("book");
  const baseUri = workspaceCtx.workspaceUri();

  const prepareMain = async (mainPath: string) => {
    const uri = workspaceCtx.workspaceUri();
    const mainUrl = vscode.Uri.joinPath(uri, mainPath);

    console.log("Start file test on ", mainUrl.fsPath);
    return await workspaceCtx.openDocument(mainUrl);
  };

  const testDesiredNewFilePath = async (
    uri: vscode.Uri,
    pasteScript: string,
    file: Pick<vscode.DataTransferFile, "name">,
  ) => {
    for (;;) {
      let commandNotFound = false;
      const res = await getDesiredNewFilePath(uri, pasteScript, file, async (uri, code) => {
        try {
          return await vscode.commands.executeCommand<any>("tinymist.interactCodeContext", {
            textDocument: { uri: uri.toString() },
            query: [
              {
                kind: "pathAt",
                code,
                inputs: {},
              },
            ],
          });
        } catch (err: any) {
          if (err.toString().includes("not found")) {
            commandNotFound = true;
          }
          throw err;
        }
      });
      if (commandNotFound) {
        console.log("probing interactCodeContext command");
        await new Promise((resolve) => setTimeout(resolve, 500));
        continue;
      }

      ctx.expect(res).to.be.ok;
      return res;
    }
  };

  const tit = (name: string) => async (uri: vscode.Uri, script: string, toBe: string) => {
    const res = await testDesiredNewFilePath(uri, script, { name });
    const x = path.resolve(res.fsPath);
    const expected = path.resolve(baseUri.fsPath, toBe);
    ctx.expect(x).to.be.eq(expected, `scriptRes=${x}, expected=${expected}`);
  };
  const pngTest = tit("test.png");

  await workspaceCtx.suite("getDesiredNewFilePath", async (suite) => {
    const editor = await prepareMain("main.typ");
    suite.addTest("assets", () => pngTest(editor.document.uri, "$root/assets", "assets/test.png"));
    suite.addTest("assets name", () =>
      pngTest(editor.document.uri, "$root/assets/$name", "assets/main/test.png"),
    );
    suite.addTest("assets dir name", () =>
      pngTest(editor.document.uri, "$root/assets/$dir/$name", "assets/main/test.png"),
    );
  });
  await workspaceCtx.suite("getDesiredNewFilePathInSubFile", async (suite) => {
    const editor = await prepareMain("chapters/chapter1.typ");
    suite.addTest("assets", () => pngTest(editor.document.uri, "$root/assets", "assets/test.png"));
    suite.addTest("assets name", () =>
      pngTest(editor.document.uri, "$root/assets/$name", "assets/chapter1/test.png"),
    );
    suite.addTest("assets dir name", () =>
      pngTest(editor.document.uri, "$root/assets/$dir/$name", "assets/chapters/chapter1/test.png"),
    );
  });
}
