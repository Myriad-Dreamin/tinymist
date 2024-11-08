import * as vscode from "vscode";

import { vscodeVariables } from "./vscode-variables";

export function loadTinymistConfig() {
  let config: Record<string, any> = JSON.parse(
    JSON.stringify(vscode.workspace.getConfiguration("tinymist")),
  );
  config.colorTheme = "light";

  const keys = Object.keys(config);
  let values = keys.map((key) => config[key]);
  values = substVscodeVarsInConfig(keys, values);
  config = {};
  for (let i = 0; i < keys.length; i++) {
    config[keys[i]] = values[i];
  }
  return config;
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
    if (COLOR_THEME.includes(k)) {
      return determineVscodeTheme();
    }
    if (STR_VARIABLES.includes(k)) {
      return substVscodeVars(value as string);
    }
    if (STR_ARR_VARIABLES.includes(k)) {
      const paths = value as string[];
      if (!paths) {
        return undefined;
      }
      return paths.map((path) => substVscodeVars(path));
    }
    return value;
  });
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

function determineVscodeTheme(): any {
  console.log("determineVscodeTheme", vscode.window.activeColorTheme.kind);
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
//     "description": "In VSCode, enable compile status meaning that the extension will show the compilation status in the status bar. Since neovim and helix don't have a such feature, it is disabled by default at the language server lebel.",
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
