import { defineEditorTool } from ".";

export default defineEditorTool({
  id: "exporter",
  command: {
    command: "tinymist.openExportTool",
    title: "Export Tool",
    tooltip: "Open Export Tool",
  },
});
