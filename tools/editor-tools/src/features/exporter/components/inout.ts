import van, { type State } from "vanjs-core";
import { lastFocusedTypstDoc } from "@/vscode";

const { div, h3, input } = van.tags;

interface InputSectionProps {
  inputPath: State<string>;
  outputPath: State<string>;
}

export const InputSection = ({ inputPath, outputPath }: InputSectionProps) => {
  return div(
    { class: "flex flex-col gap-sm" },
    // Input Path Section
    div(
      { class: "flex flex-col gap-xs" },
      h3({ class: "mb-xs" }, "Input Document"),
      input({
        class: "input",
        type: "text",
        placeholder: () => lastFocusedTypstDoc.val || "Document Path",
        value: inputPath,
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          inputPath.val = target.value;
        },
      }),
    ),
    // Output Path Section
    div(
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
    ),
  );
};
