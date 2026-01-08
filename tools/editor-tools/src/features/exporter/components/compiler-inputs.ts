import van, { type State } from "vanjs-core";

const { div, h3, button, input, span, p } = van.tags;

interface CompilerInputsProps {
  inputs: State<[string, string][]>;
}

type InputEntryState = { key: State<string>; value: State<string> };

export const CompilerInputs = ({ inputs }: CompilerInputsProps) => {
  const inputEntries = van.state<InputEntryState[]>([]);

  van.derive(() => {
    inputs.val = inputEntries.val
      .filter((entry) => entry.key.val)
      .map((entry) => [entry.key.val, entry.value.val]);
    if (import.meta.env.DEV) {
      console.log("Compiler inputs updated:", inputs.val);
    }
  });

  // Shared drag state for all input entries
  let dragStartIndex = -1;

  const handleReorder = (fromIndex: number, toIndex: number) => {
    const newInputs = [...inputEntries.val];
    const [movedItem] = newInputs.splice(fromIndex, 1);
    newInputs.splice(toIndex, 0, movedItem);
    inputEntries.val = newInputs;
  };

  const createInputEntry = (entry: InputEntryState, index: number) => {
    return InputEntryComponent({
      entries: inputEntries,
      entry,
      index,
      onReorder: handleReorder,
      dragState: {
        dragStartIndex: () => dragStartIndex,
        setDragStartIndex: (idx: number) => {
          dragStartIndex = idx;
        },
      },
    });
  };

  return div(
    { class: "card" },

    div(
      { class: "flex flex-row items-center justify-between mb-sm" },
      h3("Compiler Inputs"),
      button(
        {
          class: "btn btn-icon",
          title: "Add new input entry",
          onclick: () => {
            const newInputs = [...inputEntries.val];
            newInputs.push({ key: van.state(""), value: van.state("") });
            inputEntries.val = newInputs;
          },
        },
        "＋",
      ),
    ),

    () => {
      const entries = inputEntries.val;

      return entries.length === 0
        ? p({ class: "text-desc text-center" }, "No compiler inputs defined. Click ＋ to add one.")
        : div(
            { class: "flex flex-col gap-sm" },
            ...entries.map((entry, index) => createInputEntry(entry, index)),
          );
    },
  );
};

interface InputEntryProps {
  entries: State<InputEntryState[]>;
  entry: InputEntryState;
  index: number;
  onReorder: (fromIndex: number, toIndex: number) => void;
  dragState: { dragStartIndex: () => number; setDragStartIndex: (idx: number) => void };
}

const InputEntryComponent = ({ entries, entry, index, onReorder, dragState }: InputEntryProps) => {
  const { key: keyState, value: valueState } = entry;

  const removeEntry = () => {
    const newInputs = [...entries.val];
    newInputs.splice(index, 1);
    entries.val = newInputs;
  };

  // Drag and drop handlers
  const handleDragStart = (e: DragEvent) => {
    dragState.setDragStartIndex(index);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/html", ""); // Required for Firefox
    }
    // Add visual feedback
    (e.target as HTMLElement).classList.add("dragging");
  };

  const handleDragEnd = (e: DragEvent) => {
    // Remove visual feedback
    (e.target as HTMLElement).classList.remove("dragging");
    dragState.setDragStartIndex(-1);
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = "move";
    }
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    const fromIndex = dragState.dragStartIndex();
    const toIndex = index;
    if (fromIndex !== toIndex && fromIndex >= 0) {
      onReorder(fromIndex, toIndex);
    }
    dragState.setDragStartIndex(-1);
  };

  return div(
    {
      class: "inputs-entry flex flex-row items-center gap-sm",
      draggable: true,
      ondragstart: handleDragStart,
      ondragend: handleDragEnd,
      ondragover: handleDragOver,
      ondrop: handleDrop,
    },

    // Drag handle
    div({ class: "drag-handle cursor-move text-desc" }, "⋮⋮"),

    // Key input
    input({
      class: "input flex-1",
      type: "text",
      placeholder: "Key",
      value: () => keyState.val,
      oninput: (e: Event) => {
        const target = e.target as HTMLInputElement;
        keyState.val = target.value;
      },
    }),

    // Separator
    span({ class: "text-desc select-none" }, "→"),

    // Value input
    input({
      class: "input flex-1",
      type: "text",
      placeholder: "Value",
      value: () => valueState.val,
      oninput: (e: Event) => {
        const target = e.target as HTMLInputElement;
        valueState.val = target.value;
      },
    }),

    // Remove button
    button(
      {
        class: "btn btn-secondary btn-icon text-error",
        title: "Remove this input entry",
        onclick: removeEntry,
      },
      "✕",
    ),
  );
};
