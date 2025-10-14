import van, { type State } from "vanjs-core";
import { EXPORT_FORMATS, getDefaultOptions } from "../config/formats";
import type { ExportConfig, ExportFormat } from "../types";

const { div, span } = van.tags;

interface FormatSelectorProps {
  exportConfig: State<ExportConfig>;
}

export const FormatSelector =
  ({ exportConfig }: FormatSelectorProps) =>
  () => {
    const selectedFormat = exportConfig.val.format;

    const handleFormatSelect = (format: ExportFormat) => {
      exportConfig.val = {
        ...exportConfig.val,
        format,
        options: getDefaultOptions(format),
      };
    };

    return div(
      { class: "format-selector" },
      ...EXPORT_FORMATS.map((format) =>
        FormatCard(format, selectedFormat.id === format.id, () => handleFormatSelect(format)),
      ),
    );
  };

const FormatCard = (format: ExportFormat, isSelected: boolean, onSelect: () => void) => {
  return div(
    {
      class: `format-card ${isSelected ? "selected" : ""}`,
      onclick: onSelect,
    },
    div(
      { class: "flex justify-between items-center" },
      span({ class: "font-semibold" }, format.label),
      span({ class: "badge font-mono" }, `.${format.fileExtension}`),
    ),
  );
};
