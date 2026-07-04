import * as lc from "vscode-languageclient";
import * as vscode from "vscode";
import { applySnippetTextEdits } from "./snippets";
import { activeTypstEditor } from "./util";
import { extensionState } from "./state";
import { tinymist } from "./lsp";

/**
 * A parameter literal used in requests to pass a text document and a range inside that
 * document.
 */
export interface OnEnterParams {
  /**
   * The text document.
   */
  textDocument: lc.TextDocumentIdentifier;
  /**
   * The range inside the text document.
   */
  range: lc.Range;
}

const onEnter = new lc.RequestType<OnEnterParams, lc.TextEdit[], void>("experimental/onEnter");

export async function onEnterHandler() {
  try {
    if (await handleKeypress()) return;
  } catch (e) {
    console.error("onEnter failed", e);
  }

  await vscode.commands.executeCommand("default:type", { text: "\n" });
}

// The code copied from https://github.com/rust-lang/rust-analyzer/blob/fc98e0657abf3ce07eed513e38274c89bbb2f8ad/editors/code/src/commands.ts#L199
// doesn't work, so we change `onEnter` to pass the `range` instead of `position` to the server.
async function handleKeypress() {
  if (!extensionState.features.onEnter) return false;

  const editor = activeTypstEditor();
  if (!editor) {
    return false;
  }

  const client = tinymist.client;
  if (!tinymist.checkServerHealth() || !client) {
    return false;
  }

  // When the server is not ready, skip the onEnter handling.
  const serverReady = extensionState.mut.serverReady;
  if (!serverReady) {
    console.log("onEnter: waiting for server to be ready...");
    return false;
  }

  // Prepare a cancellation token so we can cancel the request if it times out.
  const cts = new vscode.CancellationTokenSource();
  const timeout = setTimeout(() => cts.cancel(), 1000);

  let lcEdits: lc.TextEdit[] | null = null;
  try {
    lcEdits = await client.sendRequest(
      onEnter,
      {
        textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(editor.document),
        range: client.code2ProtocolConverter.asRange(editor.selection),
      },
      cts.token,
    );
  } catch (err) {
    console.debug("onEnter request failed or was cancelled", err);
  } finally {
    clearTimeout(timeout);
    cts.dispose();
  }

  if (!lcEdits) return false;

  const edits = await client.protocol2CodeConverter.asTextEdits(lcEdits);
  await applySnippetTextEdits(editor, edits);
  return true;
}
