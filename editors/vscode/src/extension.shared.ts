import { type ExtensionContext, commands } from "vscode";
import * as vscode from "vscode";

import { loadTinymistConfig } from "./config";
import { tinymist } from "./lsp";
import { extensionState } from "./state";

import { previewPreload } from "./features/preview";
import { onEnterHandler } from "./lsp.on-enter";

/**
 * The condition
 */
type FeatureCondition = boolean;
/**
 * The initialization vector
 */
type ActivationVector = (context: ExtensionContext) => void;
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
  let configWordSeparators = async () => {
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
  tinymist.configureLanguage(config["typingContinueCommentsOnNewline"]);
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("tinymist.typingContinueCommentsOnNewline")) {
        const config = loadTinymistConfig();
        // Update language configuration
        tinymist.configureLanguage(config["typingContinueCommentsOnNewline"]);
      }
    }),
  );
}

interface TinymistTrait {
  activateTable(): FeatureEntry[];
  config: Record<string, any>;
}

export async function tinymistActivate(
  context: ExtensionContext,
  trait: TinymistTrait,
): Promise<void> {
  const { activateTable, config } = trait;
  tinymist.context = context;

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
    tinymist.initClient(config);
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
      activate(context);
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
  for (const [condition, deactivate] of trait.activateTable()) {
    if (condition) {
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
