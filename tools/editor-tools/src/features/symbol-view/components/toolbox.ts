import van, { type State } from "vanjs-core";

const { div, input, label, span } = van.tags;

export const SearchBar = (onUpdate: (value: string) => void) => {
  return input({
    class: "input flex",
    placeholder: "Search symbols...",
    oninput: (e: InputEvent) => onUpdate((e.target as HTMLInputElement).value),
  });
};

export const ViewModeToggle = (showSymbolDetails: State<boolean>) => {
  return div(
    { class: "flex items-center gap-sm" },
    label(
      { class: "flex items-center gap-xs cursor-pointer" },
      input({
        type: "checkbox",
        class: "toggle",
        checked: () => showSymbolDetails.val,
        onchange: (e: Event) => {
          showSymbolDetails.val = (e.target as HTMLInputElement).checked;
        },
      }),
      span({ class: "text-sm" }, "Show symbol details"),
    ),
  );
};
