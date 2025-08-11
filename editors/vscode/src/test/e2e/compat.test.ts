/**
 * Integration test for paste API compatibility
 */
import * as vscode from "vscode";
import type { Context } from ".";
import { copyAndPasteActivate, dragAndDropActivate } from "../../features/drop-paste";
import type { IContext } from "../../context";

export async function getTests(ctx: Context) {
  let context: Pick<IContext, "subscriptions"> = { subscriptions: [] };

  const workspaceCtx = ctx.workspaceCtx("export");
  await workspaceCtx.suite("export", async (suite) => {
    // const uri = workspaceCtx.workspaceUri();
    const uri = workspaceCtx.getWorkspace("export");
    vscode.window.showInformationMessage("Start export tests.");

    console.log("Start all tests on ", uri.fsPath);

    suite.addTest("Paste API Compatibility Integration", async () => {
      it("should handle missing paste API gracefully", () => {
        // Test with a mock vscode that doesn't have the paste API
        const originalRegisterPaste = vscode.languages.registerDocumentPasteEditProvider;

        // Remove the paste API to simulate older VS Code
        delete (vscode.languages as any).registerDocumentPasteEditProvider;

        const length = context.subscriptions.length;

        // Should not throw
        ctx
          .expect(() => copyAndPasteActivate(context))
          .not.throw("registerDocumentPasteEditProvider is not usable");

        // Should not register any providers
        ctx.expect(context.subscriptions.length).to.equals(length);

        // Restore the API
        (vscode.languages as any).registerDocumentPasteEditProvider = originalRegisterPaste;
      });

      it("should work normally when paste API is available", () => {
        // Test with normal vscode that has the paste API
        if ("registerDocumentPasteEditProvider" in vscode.languages) {
          const length = context.subscriptions.length;
          ctx
            .expect(() => copyAndPasteActivate(context))
            .not.throw("registerDocumentPasteEditProvider should be usable");

          // Should register paste providers
          ctx.expect(context.subscriptions.length).to.equals(length + 2);
        }
      });
    });
  });
}
