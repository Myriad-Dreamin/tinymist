import { ExtensionContext, ExtensionMode } from "vscode";
import { TinymistConfig } from "./config";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";
import type { BaseLanguageClient as CommonClient } from "vscode-languageclient";
import { tinymist } from "./lsp";

// todo: guide installation?
export async function createSystemLanguageClient(
  context: ExtensionContext,
  config: TinymistConfig,
  clientOptions: LanguageClientOptions,
): Promise<CommonClient> {
  if (!config.probedServerPath) {
    const executable = tinymist.probeEnvPath("tinymist.serverPath", config.serverPath);
    config.probedServerPath = executable;
  }

  if (!config.probedServerPath) {
    return Promise.reject(new Error("Tinymist server path is not found."));
  }

  const isProdMode = context.extensionMode === ExtensionMode.Production;

  /// The `--mirror` flag is only used in development/test mode for testing
  const mirrorFlag = isProdMode ? [] : ["--mirror", "tinymist-lsp.log"];
  /// Set the `RUST_BACKTRACE` environment variable to `full` to print full backtrace on error. This is useless in
  /// production mode because we don't put the debug information in the binary.
  ///
  /// Note: Developers can still download the debug information from the GitHub Releases and enable the backtrace
  /// manually by themselves.
  const RUST_BACKTRACE = isProdMode ? "1" : "full";

  const run = {
    command: config.probedServerPath,
    args: ["lsp", ...mirrorFlag],
    options: { env: Object.assign({}, process.env, { RUST_BACKTRACE }) },
  };
  // console.log("use arguments", run);
  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  const client = new LanguageClient(
    "tinymist",
    "Tinymist Typst Language Server",
    serverOptions,
    clientOptions,
  );

  return client;
}
