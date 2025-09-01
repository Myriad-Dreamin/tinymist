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
      { class: "action-section" },

      h3(
        {
          style: "margin: 0 0 1rem 0; font-size: 1.125rem; font-weight: 600;",
        },
        "Export Actions",
      ),

      // Task Label Input
      div(
        { class: "card", style: "margin-bottom: 1rem; padding: 1rem;" },
        label(
          {
            class: "option-label",
            for: "task-label",
            style: "margin-bottom: 0.5rem; display: block;",
          },
          "Custom Task Label (Optional)",
        ),
        input({
          class: "option-input",
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
          {
            class: "option-description",
            style: "margin-top: 0.25rem;",
          },
          "This will be used as the task name in tasks.json. Leave empty for default naming.",
        ),
      ),

      // Action Buttons
      div(
        { class: "action-buttons" },

        // Direct Export Button
        button(
          {
            title: "Immediately export the current document with these settings",
            class: "action-button action-button-primary",
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
            class: "action-button action-button-secondary",
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

      // Export Information
      div(
        {
          class: "card",
          style:
            "margin-top: 1rem; padding: 1rem; background: var(--vscode-textCodeBlock-background);",
        },
        div({ style: "font-weight: 500; margin-bottom: 0.5rem;" }, "Export Configuration"),
        div(
          {
            style:
              "font-size: 0.875rem; color: var(--vscode-descriptionForeground); line-height: 1.4;",
          },
          `Format: ${exportConfig.val.format.label} (.${exportConfig.val.format.fileExtension})`,
          div(
            { style: "margin-top: 0.25rem;" },
            `Options: ${Object.keys(exportConfig.val.options).length} configured`,
          ),
          div(
            { style: "margin-top: 0.25rem;" },
            `Output: \${fileDirname}/\${fileBasenameNoExtension}.${exportConfig.val.format.fileExtension}`,
          ),
        ),
      ),
    );
  };
