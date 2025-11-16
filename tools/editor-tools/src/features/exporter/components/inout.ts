import van, { type ChildDom, type State } from "vanjs-core";
import { lastFocusedTypstDoc } from "@/vscode";

const { div, h3, input } = van.tags;

interface InputSectionProps {
  inputPath: State<string>;
  outputPath: State<string>;
  actionButton?: ChildDom;
}

export const InputSection = ({ inputPath, actionButton }: InputSectionProps) => {
  return div(
    { class: "flex flex-col gap-sm" },
    h3(
      { class: "mb-xs", title: "Configure and export your Typst documents to various formats" },
      "Export Document",
    ),
    // Input Path Section
    div(
      { class: "flex flex-row items-center gap-sm" },
      input({
        class: "input flex-1",
        type: "text",
        placeholder: () => lastFocusedTypstDoc.val || "Document Path",
        value: inputPath,
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          inputPath.val = target.value;
        },
      }),
      actionButton,
    ),
    // Output Path Section (not supported yet)
    /* div(
      { class: "flex flex-col gap-xs" },
      h3({ class: "mb-xs" }, "Output Path"),
      input({
        class: "input",
        type: "text",
        placeholder: "Automatically decided based on input path",
        value: outputPath,
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          outputPath.val = target.value;
        },
      }),
    ), */
  );
};
