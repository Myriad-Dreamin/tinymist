import * as vscode from "vscode";
import { ProviderResult } from "vscode";
import { IContext } from "./context";

export class DebugAdapterExecutableFactory implements vscode.DebugAdapterDescriptorFactory {
  // static outputChannel = vscode.window.createOutputChannel("Tinymist Debugging", "log");

  constructor(private readonly context: IContext) {}

  // The following use of a DebugAdapter factory shows how to control what debug adapter executable is used.
  // Since the code implements the default behavior, it is absolutely not necessary and we show it here only for educational purpose.
  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): ProviderResult<vscode.DebugAdapterDescriptor> {
    const isProdMode = this.context.context.extensionMode === vscode.ExtensionMode.Production;

    const hasMirrorFlag = () => {
      return executable?.args?.some((arg) => arg.startsWith("--mirror=") || arg === "--mirror");
    };

    /// The `--mirror` flag is only used in development/test mode for testing
    const mirrorFlag = isProdMode ? [] : hasMirrorFlag() ? [] : ["--mirror", "tinymist-dap.log"];
    /// Set the `RUST_BACKTRACE` environment variable to `full` to print full backtrace on error. This is useless in
    /// production mode because we don't put the debug information in the binary.
    ///
    /// Note: Developers can still download the debug information from the GitHub Releases and enable the backtrace
    /// manually by themselves.
    const RUST_BACKTRACE = isProdMode ? "1" : "full";

    const args = executable?.args?.length
      ? [...executable.args, ...mirrorFlag]
      : ["dap", ...mirrorFlag];

    const command = executable?.command || this.context.tinymistExecutable;

    console.log("dap executable", executable, "=>", command, args);

    if (!command) {
      vscode.window.showErrorMessage("Cannot find tinymist executable to debug");
      return;
    }

    // todo: resolve the cwd according to the program being debugged
    const cwd =
      executable?.options?.cwd ||
      session.workspaceFolder?.uri.fsPath ||
      (vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders.length > 0
        ? vscode.workspace.workspaceFolders[0].uri.fsPath
        : undefined);

    return new vscode.DebugAdapterExecutable(command, args, {
      cwd,
      env: {
        ...(executable?.options?.env || {}),
        RUST_BACKTRACE,
      },
    });
  }
}
