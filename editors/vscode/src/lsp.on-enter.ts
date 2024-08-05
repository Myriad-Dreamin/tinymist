import * as lc from "vscode-languageclient";
import * as vscode from "vscode";
import { client } from "./lsp";
import { applySnippetTextEdits } from "./snippets";
import { activeTypstEditor } from "./util";
import { extensionState } from "./state";

const onEnter = new lc.RequestType<lc.TextDocumentPositionParams, lc.TextEdit[], void>(
  "experimental/onEnter",
);

export function onEnterHandler() {
  async function handleKeypress() {
    if (!extensionState.features.onEnter) return false;

    const editor = activeTypstEditor();

    if (!editor || !client) return false;

    const lcEdits = await client
      .sendRequest(onEnter, {
        textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(editor.document),
        position: client.code2ProtocolConverter.asPosition(editor.selection.active),
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

  return async () => {
    try {
      if (await handleKeypress()) return;
    } catch (e) {
      console.error("onEnter failed", e);
    }

    await vscode.commands.executeCommand("default:type", { text: "\n" });
  };
}
