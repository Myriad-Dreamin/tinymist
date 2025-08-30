import van, { type State } from "vanjs-core";
import type { ExportFormat, ExportConfig } from "../types";
import { EXPORT_FORMATS, getDefaultOptions } from "../config/formats";

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
      { class: "format-card-header items-center" },
      span({ class: "format-title" }, format.label),
      span({ class: "format-extension" }, `.${format.fileExtension}`),
    ),
  );
};
