import * as vscode from "vscode";
import { IContext } from "../context";
import { VirtualConsole } from "../util";

export function testingFeatureActivate(context: IContext) {
  context.registerFileLevelCommand({
    command: "tinymist.profileCurrentFileCoverage",
    execute: async (ctx) => {
      if (!context.isValidEditor(ctx.currentEditor)) {
        return;
      }
      const document = ctx.currentEditor.document;

      const executable = context.tinymistExec;
      if (!executable) {
        context.showErrorMessage("tinymist executable not found");
        return;
      }

      const vc = new VirtualConsole();
      const killer = new vscode.EventEmitter<void>();

      const terminal = vscode.window.createTerminal({
        name: "document Coverage",
        pty: {
          onDidWrite: vc.writeEmitter.event,
          open: () => {},
          close: () => {
            killer.fire();
          },
        },
      });
      terminal.show(true);

      const res = await executable.execute(
        {
          ...ctx,
          killer,
          isTTY: true,
          stdout: (data: Buffer) => {
            vc.write(data.toString("utf8"));
          },
          stderr: (data: Buffer) => {
            vc.write(data.toString("utf8"));
          },
        },
        ["cov", document.uri.fsPath],
      );

      if (!res) {
        return;
      }

      const { code } = res;
      vc.write(`\nCoverage profiling exited with code ${code}...`);
    },
  });
}
