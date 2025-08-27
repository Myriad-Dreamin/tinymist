import * as vscode from "vscode";
import { tinymist } from "../../../lsp";
import { activeTypstEditor, isTypstDocument, substituteTemplateString } from "../../../util";
import type { EditorTool } from "..";

interface InteractCodeContextResponse {
  // biome-ignore lint/suspicious/noExplicitAny: any style
  style: any[];
}

export default {
  id: "font-view",
  command: {
    command: "tinymist.showFontView",
    title: "Fonts",
    tooltip: "Show Font View",
  },
  webviewPanelOptions: {
    enableFindWidget: true,
  },

  transformHtml: async (html, { postMessage, addDisposable }) => {
    const result = await tinymist.getResource("/fonts");

    if (!result) {
      vscode.window.showErrorMessage("No resource");
      return;
    }

    const fontInfo = JSON.stringify(result);
    html = substituteTemplateString(html, { ":[[preview:FontInformation]]:": fontInfo });

    let version = 0;

    const processSelections = async (
      selectionVersion: number,
      textEditor: vscode.TextEditor | undefined,
      selections: readonly vscode.Selection[],
    ) => {
      console.log(selections);
      // todo: very buggy so we disabling it
      if (!textEditor || selections.length >= 0) {
        return;
      }

      for (const sel of selections) {
        console.log(textEditor, sel.start);
        const textDocument = {
          uri: textEditor.document.uri.toString(),
        };
        const position = {
          line: sel.start.line,
          character: sel.start.character,
        };
        const style = ["text.font"];
        const styleAt = (
          await vscode.commands.executeCommand<InteractCodeContextResponse[]>(
            "tinymist.interactCodeContext",
            {
              textDocument,
              query: [{ kind: "styleAt", position, style }],
            },
          )
        )?.[0]?.style;

        return {
          version: selectionVersion,
          selections: [{ textDocument, position, style, styleAt }],
        };
      }
    };

    addDisposable(
      vscode.window.onDidChangeTextEditorSelection(async (event) => {
        if (!isTypstDocument(event.textEditor.document)) {
          return;
        }
        version += 1;
        const styleAtCursor = await processSelections(version, event.textEditor, event.selections);

        postMessage({ type: "styleAtCursor", data: styleAtCursor });
      }),
    );

    const activeEditor = activeTypstEditor();
    if (activeEditor) {
      const styleAtCursor = await processSelections(version, activeEditor, activeEditor.selections);
      if (styleAtCursor) {
        html = substituteTemplateString(html, {
          ":[[preview:StyleAtCursor]]:": JSON.stringify(styleAtCursor),
        });
      }
    }

    return html;
  },
} satisfies EditorTool;
