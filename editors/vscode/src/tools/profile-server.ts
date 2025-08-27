import * as vscode from "vscode";
import { defineEditorTool } from ".";

export default defineEditorTool({
  id: "profile-server",
  command: {
    command: "tinymist.profileServer",
    title: "Profiling Server",
    tooltip: "Profile the Language Server",
  },
  showOption: {
    preserveFocus: true,
  },

  postLoadHtml: async ({ postMessage }) => {
    const profileHookPromise = vscode.commands.executeCommand("tinymist.startServerProfiling");

    // do that after the html is reloaded
    const profileHook = await profileHookPromise;
    postMessage({ type: "didStartServerProfiling", data: profileHook });
  },
});
