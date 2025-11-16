import van from "vanjs-core";

const { div, button } = van.tags;

interface ActionButtonsProps {
  onExport: () => void;
}

export const ActionButtons = ({ onExport }: ActionButtonsProps) => {
  return div(
    { class: "flex flex-col gap-sm" },

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
        "Export",
      ),

      // Create Task Button
      /* button(
        {
          title: "Add this export configuration to .vscode/tasks.json for reuse",
          class: "btn btn-secondary action-button",
          onclick: handleCreateTask,
        },
        "Create Task",
      ), */
    ),
  );
};
