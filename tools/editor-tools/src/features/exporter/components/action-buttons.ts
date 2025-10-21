import van from "vanjs-core";

const { div, h3, button } = van.tags;

interface ActionButtonsProps {
  onExport: () => void;
}

export const ActionButtons = ({ onExport }: ActionButtonsProps) => {
  return div(
    { class: "flex flex-col gap-sm" },

    h3("Export Actions"),

    // Task Label Input
    /*   div(
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
    ), */

    // Action Buttons
    div(
      { class: "action-buttons flex items-center gap-md" },

      // Direct Export Button
      button(
        {
          title: "Immediately export the current document with these settings",
          class: "btn action-button",
          onclick: onExport,
        },
        "📄 Export",
      ),

      // Create Task Button
      /* button(
        {
          title: "Add this export configuration to .vscode/tasks.json for reuse",
          class: "btn btn-secondary action-button",
          onclick: handleCreateTask,
        },
        "⚙️",
        "Create Task",
      ), */
    ),
  );
};
