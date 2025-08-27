import type { EditorTool } from "..";
import * as vscode from "vscode";
import { extensionState } from "../../../state";

export default {
  id: "tracing",
  command: {
    title: "Profiling",
    command: "tinymist.profileCurrentFile",
    tooltip: "Profile Current File",
  },
  showOption: {
    preserveFocus: true,
  },

  postLoadHtml: async ({ postMessage }) => {
    const focusingFile = extensionState.getFocusingFile();
    if (focusingFile === undefined) {
      await vscode.window.showErrorMessage("No focusing typst file");
      return;
    }
    const traceDataTask = vscode.commands.executeCommand("tinymist.getDocumentTrace", focusingFile);

    // do that after the html is reloaded
    const traceData = await traceDataTask;
    postMessage({ type: "traceData", data: traceData });
  },
} satisfies EditorTool;
