/**
 * Integration test for paste API compatibility
 */
import * as vscode from "vscode";
import type { Context } from ".";
import { copyAndPasteActivate } from "../../features/drop-paste";
import type { IContext } from "../../context";

export async function getTests(ctx: Context) {
  let context: Pick<IContext, "subscriptions"> = { subscriptions: [] };

  await ctx.suite("Paste API Compatibility Integration", async (suite) => {
    const workspaceUri = ctx.getWorkspace("simple-docs");
    console.log("Start all tests on ", workspaceUri.fsPath);
      

    suite.addTest("should handle missing paste API gracefully", async () => {
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

    suite.addTest("should work normally when paste API is available", async () => {
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
}
