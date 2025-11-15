import * as build from "./builders.mjs";

const kind = process.argv[2];
const vector = {
  "build:l10n": build.buildL10n,
  "build:syntax": build.buildSyntax,
  "build:preview": build.buildPreview,
  "build:editor-tools": build.buildEditorTools,
  "build:vscode:web": build.buildTinymistVscodeWeb,
  "build:vscode:system": build.buildTinymistVscodeSystem,
  "build:lsp:debug": () => build.buildLspBinary("debug"),
  "prelaunch:vscode": () => build.prelaunchVscode("debug"),
  "prelaunch:vscode-release": () => build.prelaunchVscode("release"),
  "install:vscode": () => build.installVscode("release"),
  "build:web:base": build.buildWebLspBinaryBase,
  "build:web": build.buildTinymistVscodeWeb,
  "test:vsc": build.testTinymistVscode,
};

const fn = vector[kind];
if (fn) {
  await fn();
} else {
  console.error(`Unknown command: ${kind}`);
  process.exit(1);
}
