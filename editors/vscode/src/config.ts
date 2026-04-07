import * as vscode from "vscode";

import { vscodeVariables } from "./vscode-variables";

export interface TinymistConfig {
  [key: string]: any;
  typingContinueCommentsOnNewline?: boolean;
  serverPath?: string;
}

const EXTENSION_MANAGED_TINYMIST_CONFIG = {
  triggerSuggest: true,
  triggerSuggestAndParameterHints: true,
  triggerParameterHints: true,
  supportHtmlInMarkdown: true,
  supportClientCodelens: true,
  supportExtendedCodeAction: true,
  customizedShowDocument: true,
  delegateFsRequests: false,
} as const;

export function applyExtensionManagedTinymistConfig(config: TinymistConfig): TinymistConfig {
  if (!config || typeof config !== "object" || Array.isArray(config)) {
    return { ...EXTENSION_MANAGED_TINYMIST_CONFIG };
  }
  return Object.assign(config, EXTENSION_MANAGED_TINYMIST_CONFIG);
}

export function loadTinymistConfig(): TinymistConfig {
  return normalizeTinymistConfigValue(vscode.workspace.getConfiguration("tinymist"));
}

const STR_VARIABLES = [
  "serverPath",
  "tinymist.serverPath",
  "rootPath",
  "tinymist.rootPath",
  "outputPath",
  "tinymist.outputPath",
];
const STR_ARR_VARIABLES = ["fontPaths", "tinymist.fontPaths"];
const COLOR_THEME = ["colorTheme", "tinymist.colorTheme"];

// todo: documentation that, typstExtraArgs won't get variable extended
export function substVscodeVarsInConfig(
  keys: (string | undefined)[],
  values: unknown[],
): unknown[] {
  return values.map((value, i) => {
    const k = keys[i];
    if (!k) {
      return value;
    }
    if (EXTENSION_MANAGED_TINYMIST_CONFIG.hasOwnProperty(k)) {
      return EXTENSION_MANAGED_TINYMIST_CONFIG[k as keyof typeof EXTENSION_MANAGED_TINYMIST_CONFIG];
    }
    if (k.startsWith("tinymist.")) {
      const subKey = k.substring("tinymist.".length);
      if (EXTENSION_MANAGED_TINYMIST_CONFIG.hasOwnProperty(subKey)) {
        return EXTENSION_MANAGED_TINYMIST_CONFIG[
          subKey as keyof typeof EXTENSION_MANAGED_TINYMIST_CONFIG
        ];
      }
    }
    if (k === "tinymist") {
      return normalizeTinymistConfigValue(value);
    }
    if (COLOR_THEME.includes(k)) {
      return determineVscodeTheme();
    }
    if (STR_VARIABLES.includes(k)) {
      return substVscodeVars(value as string);
    }
    if (STR_ARR_VARIABLES.includes(k)) {
      return substFontPaths(value);
    }
    return value;
  });
}

function normalizeTinymistConfigValue(configValue: unknown): TinymistConfig {
  let config: Record<string, any> = {};
  if (configValue && typeof configValue === "object" && !Array.isArray(configValue)) {
    config = JSON.parse(JSON.stringify(configValue));
  }
  config.colorTheme = "light";

  const keys = Object.keys(config);
  let values = keys.map((key) => config[key]);
  values = substVscodeVarsInConfig(keys, values);
  config = {};
  for (let i = 0; i < keys.length; i++) {
    config[keys[i]] = values[i];
  }
  return applyExtensionManagedTinymistConfig(config);
}

function substVscodeVars(str: string | null | undefined): string | undefined {
  if (str === undefined || str === null) {
    return undefined;
  }
  try {
    return vscodeVariables(str);
  } catch (e) {
    console.error("failed to substitute vscode variables", e);
    return str;
  }
}

function substFontPaths(value: unknown): string[] | undefined {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (!isStringArray(value)) {
    const invalidFontPathsShape = Array.isArray(value)
      ? "an array with non-string entries"
      : typeof value;
    void vscode.window.showErrorMessage(
      `Tinymist ignored "tinymist.fontPaths" setting because it must be an array of strings, for example \`["fonts"]\`. Received ${invalidFontPathsShape}.`,
    );
    return undefined;
  }
  return value.map((path) => substVscodeVars(path) ?? path);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((entry) => typeof entry === "string");
}

function determineVscodeTheme(): any {
  // console.log("determineVscodeTheme", vscode.window.activeColorTheme.kind);
  switch (vscode.window.activeColorTheme.kind) {
    case vscode.ColorThemeKind.Dark:
    case vscode.ColorThemeKind.HighContrast:
      return "dark";
    default:
      return "light";
  }
}

// "tinymist.hoverPeriscope": {
//     "title": "Show preview document in periscope mode on hovering",
//     "description": "In VSCode, enable compile status meaning that the extension will show the compilation status in the status bar. Since Neovim and helix don't have a such feature, it is disabled by default at the language server label.",
//     "type": [
//         "object",
//         "string"
//     ],
//     "default": "disable",
//     "enum": [
//         "enable",
//         "disable"
//     ],
//     "properties": {
//         "yAbove": {
//             "title": "Y above",
//             "description": "The distance from the top of the screen to the top of the periscope hover.",
//             "type": "number",
//             "default": 55
//         },
//         "yBelow": {
//             "title": "Y below",
//             "description": "The distance from the bottom of the screen to the bottom of the periscope hover.",
//             "type": "number",
//             "default": 55
//         },
//         "scale": {
//             "title": "Scale",
//             "description": "The scale of the periscope hover.",
//             "type": "number",
//             "default": 1.5
//         },
//         "invertColors": {
//             "title": "Invert colors",
//             "description": "Invert the colors of the periscope to hover.",
//             "type": "string",
//             "enum": [
//                 "auto",
//                 "always",
//                 "never"
//             ],
//             "default": "auto"
//         }
//     }
// },
