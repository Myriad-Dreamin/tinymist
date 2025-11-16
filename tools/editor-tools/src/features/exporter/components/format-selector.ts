import van, { type State } from "vanjs-core";
import { EXPORT_FORMATS } from "../formats";
import type { ExportFormat } from "../types";

const { div, span } = van.tags;

interface FormatSelectorProps {
  selectedFormat: State<ExportFormat>;
}

export const FormatSelector = ({ selectedFormat }: FormatSelectorProps) => {
  const handleFormatSelect = (format: ExportFormat) => {
    selectedFormat.val = format;
  };

  return div(
    { class: "format-selector" },
    ...EXPORT_FORMATS.map(
      (format) => () =>
        FormatCard(format, selectedFormat.val.id === format.id, () => handleFormatSelect(format)),
    ),
  );
};

const FormatCard = (format: ExportFormat, isSelected: boolean, onSelect: () => void) => {
  return div(
    {
      class: `format-card ${isSelected ? "selected" : ""}`,
      title: format.label,
      onclick: onSelect,
    },
    div(
      { class: "flex justify-between items-center" },
      span({ class: "badge font-mono" }, `.${format.fileExtension}`),
    ),
  );
};
