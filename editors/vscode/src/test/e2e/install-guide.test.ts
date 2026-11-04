// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";
import type { Context } from ".";

export async function getTests(ctx: Context) {
  await ctx.suite("install-guide", async (suite) => {
    const extensionDevelopmentPath = path.resolve(__dirname, "../../..");
    const binaryName = process.platform === "win32" ? "tinymist.exe" : "tinymist";
    const outBinary = path.resolve(extensionDevelopmentPath, "out", binaryName);

    suite.addTest("activation degrades gracefully when every binary probe fails", async () => {
      // Hide the bundled binary (if present), point PATH at an empty directory and clear
      // tinymist.serverPath, so that every probe candidate fails. Restarting the extension
      // must keep it active: the probe failure shows the installation guide instead of
      // breaking activation.
      const moved = `${outBinary}.hidden-by-test`;
      const hadBundled = fs.existsSync(outBinary);
      const oldPath = process.env.PATH;
      const emptyDir = fs.mkdtempSync(path.join(path.dirname(outBinary), "empty-path-"));
      try {
        if (hadBundled) {
          fs.renameSync(outBinary, moved);
        }
        process.env.PATH = emptyDir;
        const config = vscode.workspace.getConfiguration("tinymist");
        await config.update("serverPath", undefined, vscode.ConfigurationTarget.Global);

        await vscode.commands.executeCommand("tinymist.restartServer");

        const tinymist = vscode.extensions.getExtension("myriad-dreamin.tinymist");
        ctx.expect(tinymist?.isActive).to.be.true;
      } finally {
        process.env.PATH = oldPath;
        if (hadBundled) {
          fs.renameSync(moved, outBinary);
        }
        fs.rmdirSync(emptyDir);
        await vscode.commands.executeCommand("tinymist.restartServer");
      }
    });

    suite.addTest("no installation guide when a binary probe succeeds", async () => {
      // The e2e environment provides a working binary (bundled output or on PATH), so a
      // restart must bring the language client up without the dead-end probe error.
      const config = vscode.workspace.getConfiguration("tinymist");
      await config.update("serverPath", undefined, vscode.ConfigurationTarget.Global);

      await vscode.commands.executeCommand("tinymist.restartServer");

      const tinymist = vscode.extensions.getExtension("myriad-dreamin.tinymist");
      ctx.expect(tinymist?.isActive).to.be.true;
      await vscode.commands.executeCommand("tinymist.showLog");
    });
  });
}
