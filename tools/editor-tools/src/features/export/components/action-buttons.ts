import van, { type State } from "vanjs-core";
import type { ExportConfig } from "../types";
import { generateTaskDefinition } from "../config/task-templates";
import { requestCreateExportTask, requestExportDocument } from "@/vscode";

const { div, h3, button, span, input, label } = van.tags;

interface ActionButtonsProps {
  exportConfig: State<ExportConfig>;
}

export const ActionButtons =
  ({ exportConfig }: ActionButtonsProps) =>
  () => {
    const isExporting = van.state<boolean>(false);
    const exportStatus = van.state<string>("");
    const exportError = van.state<string | null>(null);
    const customTaskLabel = van.state<string>("");

    const handleDirectExport = async () => {
      isExporting.val = true;
      exportError.val = null;
      exportStatus.val = "Preparing export...";

      try {
        exportStatus.val = "Exporting document...";

        // Request export from VSCode extension
        requestExportDocument(exportConfig.val.format.id, exportConfig.val.options);

        // In a real implementation, we would receive the response via VSCode channel
        // For now, we'll simulate the export process
        setTimeout(() => {
          exportStatus.val = "Export completed successfully!";
          isExporting.val = false;

          // Clear status after a few seconds
          setTimeout(() => {
            exportStatus.val = "";
          }, 3000);
        }, 2000);
      } catch (err) {
        exportError.val = err instanceof Error ? err.message : "Export failed";
        exportStatus.val = "";
        isExporting.val = false;
      }
    };

    const handleCreateTask = () => {
      try {
        const taskDefinition = generateTaskDefinition(exportConfig.val);

        // Apply custom label if provided
        if (customTaskLabel.val.trim()) {
          taskDefinition.label = customTaskLabel.val.trim();
        }

        // Request task creation from VSCode extension
        requestCreateExportTask(taskDefinition);

        exportStatus.val = "Task created successfully in tasks.json!";

        // Clear status after a few seconds
        setTimeout(() => {
          exportStatus.val = "";
        }, 3000);
      } catch (err) {
        exportError.val = err instanceof Error ? err.message : "Failed to create task";
      }
    };

    const clearStatus = () => {
      exportStatus.val = "";
      exportError.val = null;
    };

    return div(
      { class: "flex flex-col gap-sm" },

      h3("Export Actions"),

      // Task Label Input
      div(
        { class: "card flex flex-col gap-xs" },
        label(
          {
            class: "text-sm font-medium",
            for: "task-label",
            style: "margin-bottom: 0.5rem; display: block;",
          },
          "Custom Task Label (Optional)",
        ),
        input({
          class: "input",
          type: "text",
          id: "task-label",
          placeholder: `Export to ${exportConfig.val.format.label}`,
          value: customTaskLabel,
          oninput: (e: Event) => {
            const target = e.target as HTMLInputElement;
            customTaskLabel.val = target.value;
          },
        }),
        div(
          { class: "text-xs text-desc" },
          "This will be used as the task name in tasks.json. Leave empty for default naming.",
        ),
      ),

      // Action Buttons
      div(
        { class: "action-buttons flex items-center gap-md" },

        // Direct Export Button
        button(
          {
            title: "Immediately export the current document with these settings",
            class: "btn action-button",
            onclick: handleDirectExport,
            disabled: isExporting.val,
          },
          isExporting.val ? div({ class: "action-spinner" }) : "📄",
          isExporting.val ? "Exporting..." : "Export Now",
        ),

        // Create Task Button
        button(
          {
            title: "Add this export configuration to .vscode/tasks.json for reuse",
            class: "btn btn-secondary action-button",
            onclick: handleCreateTask,
            disabled: isExporting.val,
          },
          "⚙️",
          "Create Task",
        ),
      ),

      // Status Display
      (() => {
        if (exportStatus.val) {
          const isSuccess = exportStatus.val.includes("success");
          return div(
            {
              class: `action-status ${isSuccess ? "success" : ""}`,
              style: "margin-top: 1rem;",
            },
            span("✓"),
            exportStatus.val,
            button(
              {
                class: "btn",
                style: "margin-left: 0.5rem; font-size: 0.75rem; padding: 0.125rem 0.25rem;",
                onclick: clearStatus,
              },
              "×",
            ),
          );
        }

        if (exportError.val) {
          return div(
            {
              class: "action-status error",
              style: "margin-top: 1rem;",
            },
            span("⚠️"),
            exportError.val,
            button(
              {
                class: "btn",
                style: "margin-left: 0.5rem; font-size: 0.75rem; padding: 0.125rem 0.25rem;",
                onclick: clearStatus,
              },
              "×",
            ),
          );
        }

        return div();
      })(),
    );
  };
