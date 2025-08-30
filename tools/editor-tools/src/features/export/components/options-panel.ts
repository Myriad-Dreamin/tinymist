import van, { type State } from "vanjs-core";
import type { ExportConfig, OptionSchema } from "../types";

const { div, h3, label, input, select, option, span, p } = van.tags;

interface OptionsPanelProps {
  exportConfig: State<ExportConfig>;
}

export const OptionsPanel =
  ({ exportConfig }: OptionsPanelProps) =>
  () => {
    const { format, options } = exportConfig.val;

    if (format.options.length === 0) {
      return div(
        { class: "options-panel" },
        div(
          { class: "text-center", style: "padding: 2rem;" },
          h3(
            { style: "margin: 0 0 0.5rem 0; color: var(--vscode-foreground)" },
            "No Configuration Needed",
          ),
          p(
            { class: "text-desc", style: "margin: 0;" },
            `${format.label} export doesn't require additional configuration.`,
          ),
        ),
      );
    }

    const updateOption = (key: string, value: string | number | boolean | undefined) => {
      exportConfig.val = {
        ...exportConfig.val,
        options: {
          ...exportConfig.val.options,
          [key]: value,
        },
      };
    };

    return div(
      { class: "options-panel" },
      h3(
        { style: "margin: 0 0 1rem 0; color: var(--vscode-foreground)" },
        `${format.label} Options`,
      ),
      div(
        { class: "options-grid" },
        ...format.options.map((optionSchema) =>
          OptionField(optionSchema, options[optionSchema.key], (value) =>
            updateOption(optionSchema.key, value),
          ),
        ),
      ),
    );
  };

const OptionField = (
  schema: OptionSchema,
  currentValue: string | number | boolean | undefined,
  onChange: (value: string | number | boolean | undefined) => void,
) => {
  const {
    key,
    type,
    label: optionLabel,
    description,
    default: defaultValue,
    options: selectOptions,
    min,
    max,
  } = schema;

  const value = currentValue !== undefined ? currentValue : defaultValue;

  return div(
    { class: "option-group" },
    label({ class: "option-label", for: key }, optionLabel),
    renderInput(type, key, value, onChange, { selectOptions, min, max }),
    description ? p({ class: "option-description" }, description) : null,
  );
};

const renderInput = (
  type: OptionSchema["type"],
  key: string,
  value: string | number | boolean | undefined,
  onChange: (value: string | number | boolean | undefined) => void,
  props: {
    selectOptions?: Array<{ value: string | number | boolean; label: string }>;
    min?: number;
    max?: number;
  },
) => {
  const { selectOptions, min, max } = props;

  switch (type) {
    case "string":
      return input({
        class: "option-input",
        type: "text",
        id: key,
        value: String(value || ""),
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          onChange(target.value);
        },
      });

    case "number":
      return input({
        class: "option-input",
        type: "number",
        id: key,
        value: String(value || ""),
        min: min?.toString() ?? null,
        max: max?.toString() ?? null,
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          const numValue = parseFloat(target.value);
          onChange(Number.isNaN(numValue) ? undefined : numValue);
        },
      });

    case "boolean":
      return label(
        { style: "display: flex; align-items: center; cursor: pointer;" },
        input({
          class: "option-input",
          type: "checkbox",
          id: key,
          checked: !!value,
          onchange: (e: Event) => {
            const target = e.target as HTMLInputElement;
            onChange(target.checked);
          },
        }),
        span({ style: "font-size: 0.875rem;" }, "Enable"),
      );

    case "color":
      return input({
        class: "option-input",
        type: "color",
        id: key,
        value: String(value || "#ffffff"),
        onchange: (e: Event) => {
          const target = e.target as HTMLInputElement;
          onChange(target.value);
        },
      });

    case "select":
      if (!selectOptions) return span("No options available");
      return select(
        {
          class: "option-select",
          id: key,
          onchange: (e: Event) => {
            const target = e.target as HTMLSelectElement;
            const selectedOption = selectOptions.find(
              (opt) => opt.value.toString() === target.value,
            );
            onChange(selectedOption ? selectedOption.value : target.value);
          },
        },
        ...selectOptions.map((opt) =>
          option(
            {
              value: opt.value.toString(),
              selected: opt.value === value,
            },
            opt.label,
          ),
        ),
      );

    default:
      return span("Unsupported option type");
  }
};
