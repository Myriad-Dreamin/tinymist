import * as lc from "vscode-languageclient";
import * as vscode from "vscode";
import { client } from "./lsp";
import { applySnippetTextEdits } from "./snippets";
import { activeTypstEditor } from "./util";
import { extensionState } from "./state";

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

  if (!editor || !client) return false;

  const lcEdits = await client
    .sendRequest(onEnter, {
      textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(editor.document),
      range: client.code2ProtocolConverter.asRange(editor.selection),
    })
    .catch((_error: any) => {
      // client.handleFailedRequest(OnEnterRequest.type, error, null);
      return null;
    });
  if (!lcEdits) return false;

  const edits = await client.protocol2CodeConverter.asTextEdits(lcEdits);
  await applySnippetTextEdits(editor, edits);
  return true;
}
