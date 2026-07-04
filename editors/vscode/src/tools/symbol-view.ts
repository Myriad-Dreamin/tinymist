import * as vscode from "vscode";
import { tinymist } from "../lsp";
import { substituteTemplateString } from "../util";
import { defineEditorTool } from ".";

export default defineEditorTool({
  id: "symbol-view",
  command: {
    command: "tinymist.showSymbolView",
    title: "Symbols",
    tooltip: "Show Symbol View",
  },
  appDir: "symbol-view",

  transformHtml: async (html) => {
    const result = await tinymist.getResource("/symbols");

    if (!result) {
      vscode.window.showErrorMessage("No resource");
      return;
    }

    const symbolInfo = JSON.stringify(result);
    return substituteTemplateString(html, { ":[[preview:SymbolInformation]]:": symbolInfo });
  },
});
