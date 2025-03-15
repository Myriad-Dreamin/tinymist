import { type ExtensionContext, commands } from "vscode";
import * as vscode from "vscode";

import { loadTinymistConfig, TinymistConfig } from "./config";
import { tinymist } from "./lsp";
import { extensionState } from "./state";

import { previewPreload } from "./features/preview";
import { onEnterHandler } from "./lsp.on-enter";
import { ExecContext, ExecResult, ICommand, IContext } from "./context";
import { spawn } from "cross-spawn";

/**
 * The condition
 */
type FeatureCondition = boolean;
/**
 * The initialization vector
 */
type ActivationVector = (context: IContext) => void;
/**
 * The initialization vector
 */
type DeactivationVector = (context: ExtensionContext) => void;
/**
 * The feature entry. A conditional feature activation vector is required
 * and an optional deactivation vector is also supported.
 */
export type FeatureEntry =
  | [FeatureCondition, ActivationVector]
  | [FeatureCondition, ActivationVector, DeactivationVector];

function configureEditorAndLanguage(context: ExtensionContext, trait: TinymistTrait) {
  const isDevMode = vscode.ExtensionMode.Development == context.extensionMode;
  const isWeb = extensionState.features.web;
  const { config } = trait;

  // Inform server that we support named completion callback at the client side
  config.triggerSuggest = true;
  config.triggerSuggestAndParameterHints = true;
  config.triggerParameterHints = true;
  config.supportHtmlInMarkdown = true;
  // Sets shared features
  extensionState.features.preview = !isWeb && config.previewFeature === "enable";
  extensionState.features.wordSeparator = config.configureDefaultWordSeparator !== "disable";
  extensionState.features.devKit = isDevMode || config.devKit === "enable";
  extensionState.features.dragAndDrop = !isWeb && config.dragAndDrop === "enable";
  extensionState.features.copyAndPaste = !isWeb && config.copyAndPaste === "enable";
  extensionState.features.onEnter = !isWeb && !!config.onEnterEvent;
  extensionState.features.renderDocs = !isWeb && config.renderDocs === "enable";

  // Configures advanced editor settings to affect the host process
  const configWordSeparators = async () => {
    const wordSeparators = "`~!@#$%^&*()=+[{]}\\|;:'\",.<>/?";
    const config1 = vscode.workspace.getConfiguration("", { languageId: "typst" });
    await config1.update("editor.wordSeparators", wordSeparators, true, true);
    const config2 = vscode.workspace.getConfiguration("", { languageId: "typst-code" });
    await config2.update("editor.wordSeparators", wordSeparators, true, true);
  };
  // Runs configuration asynchronously to avoid blocking the activation
  if (extensionState.features.wordSeparator) {
    configWordSeparators().catch((e) =>
      console.error("cannot change editor.wordSeparators for typst", e),
    );
  } else {
    // console.log("skip configuring word separator on startup");
  }

  // Configures advanced language configuration
  tinymist.configureLanguage(config["typingContinueCommentsOnNewline"] || false);
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("tinymist.typingContinueCommentsOnNewline")) {
        const config = loadTinymistConfig();
        // Update language configuration
        tinymist.configureLanguage(config["typingContinueCommentsOnNewline"] || false);
      }
    }),
  );
}

interface TinymistTrait {
  activateTable(): FeatureEntry[];
  config: TinymistConfig;
}

export async function tinymistActivate(
  context: ExtensionContext,
  trait: TinymistTrait,
): Promise<void> {
  const { activateTable, config } = trait;
  tinymist.context = context;
  const contextExt = new IContext(context);

  // Sets a global context key to indicate that the extension is activated
  vscode.commands.executeCommand("setContext", "ext.tinymistActivated", true);
  context.subscriptions.push({
    dispose: () => {
      vscode.commands.executeCommand("setContext", "ext.tinymistActivated", false);
    },
  });

  configureEditorAndLanguage(context, trait);

  // Initializes language client
  if (extensionState.features.lsp) {
    const executable = tinymist.probeEnvPath("tinymist.serverPath", config.serverPath);
    config.probedServerPath = executable;
    // todo: guide installation?

    if (config.probedServerPath) {
      tinymist.initClient(config);
    }

    contextExt.tinymistExecutable = executable;
    contextExt.tinymistExec = makeExecCommand(contextExt, executable);
  }
  // Register Shared commands
  context.subscriptions.push(
    commands.registerCommand("tinymist.onEnter", onEnterHandler),
    commands.registerCommand("tinymist.restartServer", async () => {
      await tinymistDeactivate(trait);
      await tinymistActivate(context, trait);
    }),
    commands.registerCommand("tinymist.showLog", () => tinymist.showLog()),
  );
  // Activates platform-dependent features
  for (const [condition, activate] of activateTable()) {
    if (condition) {
      activate(contextExt);
    }
  }
  // Starts language client
  if (extensionState.features.lsp) {
    await tinymist.startClient();
  }
  // Loads the preview HTML from the binary
  if (extensionState.features.lsp && extensionState.features.preview) {
    previewPreload(context);
  }

  return;
}

export async function tinymistDeactivate(
  trait: Pick<TinymistTrait, "activateTable">,
): Promise<void> {
  for (const [condition, , deactivate] of trait.activateTable()) {
    if (deactivate && condition) {
      deactivate(tinymist.context);
    }
  }
  if (tinymist.context) {
    for (const disposable of tinymist.context.subscriptions.splice(0)) {
      disposable.dispose();
    }
  }
  await tinymist.stop();
  tinymist.context = undefined!;
}

function makeExecCommand(
  context: IContext,
  executable?: string,
): ICommand<ExecContext, Promise<ExecResult | undefined>> {
  return {
    command: "tinymist.executeCli",
    execute: async (ctx, cliArgs: string[]) => {
      if (!executable) {
        return;
      }

      const cwd = context.getCwd(ctx);
      const proc = spawn(executable, cliArgs, {
        env: {
          ...process.env,
          RUST_BACKTRACE: "1",
        },
        cwd,
      });

      if (ctx.killer) {
        ctx.killer.event(() => {
          proc.kill();
        });
      }

      const capturedStdout: Buffer[] = [];
      const capturedStderr: Buffer[] = [];

      proc.stdout.on("data", (data: Buffer) => {
        if (ctx.stdout) {
          ctx.stdout(data);
        } else {
          capturedStdout.push(data);
        }
      });
      proc.stderr.on("data", (data: Buffer) => {
        if (ctx.stderr) {
          ctx.stderr(data);
        } else {
          capturedStderr.push(data);
        }
      });

      return new Promise<ExecResult>((resolve, reject) => {
        proc.on("error", reject);
        proc.on("exit", (code: any, signal) => {
          resolve({
            stdout: Buffer.concat(capturedStdout),
            stderr: Buffer.concat(capturedStderr),
            code: code || 0,
            signal,
          });
        });
      });
    },
  };
}
