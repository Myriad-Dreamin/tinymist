import { defineEditorTool } from ".";

export default defineEditorTool({
  id: "export",
  command: {
    command: "tinymist.openExportTool",
    title: "Export Tool",
    tooltip: "Open Export Tool",
  },
});
