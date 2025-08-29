import * as vscode from "vscode";
import * as lc from "vscode-languageclient";
import { applySnippetWorkspaceEdit, SnippetTextDocumentEdit } from "./snippets";
import { tinymist } from "./lsp";
import type { LanguageClient } from "vscode-languageclient/node";

export function resolveCodeAction(): any {
  return async (params: lc.CodeAction) => {
    console.log("triggered resolveCodeAction", params);
  
    const client = await tinymist.getClient();

    let itemEdit = params.edit;
    if (!itemEdit) {
      console.log("resolving code edit", params);

      // Resolve the code edit.
      const resolvedItem = await client.sendRequest(lc.CodeActionResolveRequest.type, params);
      itemEdit = resolvedItem.edit;
    }

    // console.log("itemEdit", itemEdit);

    if (itemEdit.changes) {
      itemEdit.documentChanges ||= [];
      for (const [uri, edits] of Object.entries(itemEdit.changes)) {
        itemEdit.documentChanges.push(
          lc.TextDocumentEdit.create(lc.VersionedTextDocumentIdentifier.create(uri, 0), edits),
        );
      }
      itemEdit.changes = undefined;
    }

    console.log("itemEdit merged", itemEdit);

    // filter out all text edits and recreate the WorkspaceEdit without them so we can apply
    // snippet edits on our own
    const lcFileSystemEdit = {
      ...itemEdit,
      documentChanges: itemEdit.documentChanges?.filter((change) => "kind" in change),
    };
    const fileSystemEdit = await client.protocol2CodeConverter.asWorkspaceEdit(lcFileSystemEdit);
    await vscode.workspace.applyEdit(fileSystemEdit);

    // replace all text edits so that we can convert snippet text edits into `vscode.SnippetTextEdit`s
    // FIXME: this is a workaround until vscode-languageclient supports doing the SnippeTextEdit conversion itself
    // also need to carry the snippetTextDocumentEdits separately, since we can't retrieve them again using WorkspaceEdit.entries
    const [workspaceTextEdit, snippetTextDocumentEdits] = asWorkspaceSnippetEdit(client, itemEdit);
    console.log("applying snippet workspace edit", workspaceTextEdit, snippetTextDocumentEdits);
    await applySnippetWorkspaceEdit(workspaceTextEdit, snippetTextDocumentEdits);
    if (params.command != null) {
      await vscode.commands.executeCommand(params.command.command, params.command.arguments);
    }
  };
}
function asWorkspaceSnippetEdit(
  client: LanguageClient,
  item: lc.WorkspaceEdit,
): [vscode.WorkspaceEdit, SnippetTextDocumentEdit[]] {
  // partially borrowed from https://github.com/microsoft/vscode-languageserver-node/blob/295aaa393fda8ecce110c38880a00466b9320e63/client/src/common/protocolConverter.ts#L1060-L1101
  const result = new vscode.WorkspaceEdit();

  if (item.documentChanges) {
    const snippetTextDocumentEdits: SnippetTextDocumentEdit[] = [];

    for (const change of item.documentChanges) {
      if (lc.TextDocumentEdit.is(change)) {
        const uri = client.protocol2CodeConverter.asUri(change.textDocument.uri);
        const snippetTextEdits: (vscode.TextEdit | vscode.SnippetTextEdit)[] = [];

        for (const edit of change.edits) {
          if ("insertTextFormat" in edit && edit.insertTextFormat === lc.InsertTextFormat.Snippet) {
            // is a snippet text edit
            snippetTextEdits.push(
              new vscode.SnippetTextEdit(
                client.protocol2CodeConverter.asRange(edit.range),
                new vscode.SnippetString(edit.newText),
              ),
            );
          } else {
            // always as a text document edit
            snippetTextEdits.push(
              vscode.TextEdit.replace(
                client.protocol2CodeConverter.asRange(edit.range),
                edit.newText,
              ),
            );
          }
        }

        snippetTextDocumentEdits.push([uri, snippetTextEdits]);
      }
    }
    return [result, snippetTextDocumentEdits];
  } else {
    // we don't handle WorkspaceEdit.changes since it's not relevant for code actions
    return [result, []];
  }
}
