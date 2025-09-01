import van, { type State } from "vanjs-core";
import type { ExportConfig, OptionSchema } from "../types";
import { focusedDocUri, isDocUriLocked } from "@/vscode";

const { div, h3, label, input, select, option, span, p, button } = van.tags;

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
        ...format.options
          .filter((optionSchema) => {
            // Filter out options that depend on other options that aren't true
            if (optionSchema.dependsOn) {
              return !!options[optionSchema.dependsOn];
            }
            return true;
          })
          .map((optionSchema) =>
            OptionField(optionSchema, options[optionSchema.key], (value) =>
              updateOption(optionSchema.key, value),
            ),
          ),
      ),
    );
  };

export const DocumentUriSection = () => {
  const updateDocUri = (newUri: string) => {
    if (focusedDocUri.val) {
      focusedDocUri.val = { ...focusedDocUri.val, uri: newUri };
    } else {
      focusedDocUri.val = { version: 0, uri: newUri };
    }
  };

  const toggleLock = () => {
    isDocUriLocked.val = !isDocUriLocked.val;
  };

  return div(
    { class: "document-uri-section", style: "margin-bottom: 1.5rem;" },
    h3(
      { style: "margin: 0 0 0.5rem 0; color: var(--vscode-foreground); font-size: 1rem;" },
      "Input Document"
    ),
    div(
      { style: "display: flex; gap: 0.5rem; align-items: center;" },
      input({
        class: "option-input",
        type: "text",
        placeholder: "Document URI (auto-detected)",
        value: () => focusedDocUri.val?.uri || "",
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          updateDocUri(target.value);
        },
        style: "flex: 1;"
      }),
      button({
        class: () => `btn btn-sm ${isDocUriLocked.val ? 'btn-active' : 'btn-secondary'}`,
        onclick: toggleLock,
        title: () => isDocUriLocked.val ? "Unlock (auto-update)" : "Lock (manual input)",
        style: "padding: 0.25rem 0.5rem; font-size: 0.75rem;"
      }, () => isDocUriLocked.val ? "🔒" : "🔓")
    ),
    p(
      { class: "option-description", style: "margin: 0.25rem 0 0 0; font-size: 0.75rem;" },
      () => isDocUriLocked.val
        ? "Input locked for manual editing"
        : "Auto-updates when document focus changes"
    )
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
          // Prevent losing focus by not updating if the value hasn't actually changed
          const newValue = target.value;
          if (newValue !== (value || "")) {
            onChange(newValue);
          }
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
          const newValue = Number.isNaN(numValue) ? undefined : numValue;
          if (newValue !== value) {
            onChange(newValue);
          }
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
            const newValue = target.checked;
            if (newValue !== !!value) {
              onChange(newValue);
            }
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
          const newValue = target.value;
          if (newValue !== String(value || "#ffffff")) {
            onChange(newValue);
          }
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
            const newValue = selectedOption ? selectedOption.value : target.value;
            if (newValue !== value) {
              onChange(newValue);
            }
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
