import van, { type State } from "vanjs-core";
import type { ExportFormat, OptionSchema, Scalar } from "../types";

const { div, h3, label, input, select, option, span, p } = van.tags;

interface OptionsPanelProps {
  format: ExportFormat;
  // optionStates values can be scalar, an array (for multi-select), or undefined
  optionStates: Record<string, State<Scalar | Scalar[] | undefined>>;
}

export const OptionsPanel = ({ format, optionStates }: OptionsPanelProps) => {
  const options = format.options;
  for (const option of options) {
    if (!optionStates[option.key]) {
      optionStates[option.key] = van.state(option.default);
    }
  }

  if (options.length === 0) {
    return div(
      { class: "card" },
      div(
        { class: "text-center" },
        h3({ class: "mb-sm" }, "No Configuration Needed"),
        p(
          { class: "text-desc" },
          `${format.label} export doesn't require additional configuration.`,
        ),
      ),
    );
  }

  return div(
    { class: "card" },

    h3({ class: "mb-sm" }, ` Options`),

    div(
      { class: "options-grid" },
      ...options
        .filter((schema) => (schema.dependsOn ? optionStates[schema.dependsOn]?.val : true))
        .map((schema) => {
          const valueState = optionStates[schema.key];
          if (!valueState) {
            throw new Error(`Missing state for option ${schema.key}`);
          }
          return OptionField(schema, valueState);
        }),
    ),
  );
};

const OptionField = (schema: OptionSchema, valueState: State<Scalar | Scalar[] | undefined>) => {
  const { key, label: optionLabel, description, type: optionType } = schema;
  const validationError = van.state<string | undefined>();
  const labelElem =
    optionType !== "boolean"
      ? [label({ class: "text-sm font-medium", for: key }, optionLabel)]
      : [];

  return div(
    { class: "flex flex-col gap-xs" },
    ...labelElem,
    renderInput(schema, valueState, validationError),
    () =>
      validationError.val
        ? p({ class: "text-xs text-error" }, validationError.val)
        : p({ class: "text-xs text-desc" }, description),
  );
};

const renderInput = (
  schema: OptionSchema,
  valueState: State<Scalar | Scalar[] | undefined>,
  validationError: State<string | undefined>,
) => {
  const { type, key, options: selectOptions, label: optionLabel } = schema;

  switch (type) {
    case "string":
      return input({
        class: () => (validationError.val ? "input input-error" : "input"),
        type: "text",
        id: key,
        value: () => String((valueState.val ?? "") as string),
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          // Call custom validation function if provided
          validationError.val = schema.validate?.(target.value);
          valueState.val = target.value;
        },
      });

    case "number":
      return input({
        class: "input",
        type: "number",
        id: key,
        value: () => {
          const current = valueState.val;
          return current === undefined || current === null ? "" : String(current);
        },
        min: schema.min?.toString() ?? null,
        max: schema.max?.toString() ?? null,
        oninput: (e: Event) => {
          const target = e.target as HTMLInputElement;
          // Call custom validation function if provided
          validationError.val = schema.validate?.(target.value);
          valueState.val = target.value === "" ? undefined : parseFloat(target.value);
        },
      });

    case "boolean":
      return label(
        { class: "flex items-center cursor-pointer" },
        input({
          class: "input",
          type: "checkbox",
          id: key,
          checked: () => Boolean(valueState.val),
          onchange: (e: Event) => {
            const target = e.target as HTMLInputElement;
            valueState.val = target.checked;
          },
        }),
        label({ class: "text-sm font-medium", for: key }, optionLabel),
      );

    case "color":
      return input({
        class: "input",
        type: "color",
        id: key,
        value: () => {
          const current = valueState.val;
          return typeof current === "string" && current ? current : "#ffffff";
        },
        onchange: (e: Event) => {
          const target = e.target as HTMLInputElement;
          valueState.val = target.value;
        },
      });

    case "datetime":
      return input({
        class: "input",
        type: "datetime-local",
        id: key,
        value: () => {
          const current = valueState.val;
          return typeof current === "string" ? current : "";
        },
        onchange: (e: Event) => {
          const target = e.target as HTMLInputElement;
          valueState.val = target.value;
        },
      });

    case "select":
      if (!selectOptions) return span("No options available");
      // multi-select
      if (schema.multiple) {
        return select(
          {
            class: "select",
            id: key,
            multiple: true,
            onchange: (e: Event) => {
              const target = e.target as HTMLSelectElement;
              const values = Array.from(target.selectedOptions).map((o) => o.value);
              const resolved = values
                .map((v) => selectOptions.find((opt) => opt.value.toString() === v))
                .filter((o): o is { value: Scalar; label: string } => Boolean(o))
                .map((opt) => opt.value);
              valueState.val = resolved;
            },
          },
          ...selectOptions.map((opt) =>
            option(
              {
                value: opt.value.toString(),
                selected: () =>
                  Array.isArray(valueState.val) &&
                  (valueState.val as Scalar[])
                    .map((v) => v?.toString())
                    .includes(opt.value.toString()),
              },
              opt.label,
            ),
          ),
        );
      }

      // single-select
      return select(
        {
          class: "select",
          id: key,
          value: () => {
            const current = valueState.val;
            return current === undefined || current === null ? "" : current.toString();
          },
          onchange: (e: Event) => {
            const target = e.target as HTMLSelectElement;
            const selectedOption = selectOptions.find(
              (opt) => opt.value.toString() === target.value,
            );
            const newValue = selectedOption ? selectedOption.value : target.value;
            valueState.val = newValue;
          },
        },
        schema.default ? null : option({ value: "" }, "None"), // if no default, add a "None" option
        ...selectOptions.map((opt) =>
          option(
            {
              value: opt.value.toString(),
            },
            opt.label,
          ),
        ),
      );

    default:
      return span("Unsupported option type");
  }
};
