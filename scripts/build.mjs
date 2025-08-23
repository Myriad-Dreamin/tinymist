import * as build from "./builders.mjs";

if (process.argv.includes("build:l10n")) {
  await build.buildL10n();
}

if (process.argv.includes("build:syntax")) {
  await build.buildSyntax();
}

if (process.argv.includes("build:preview")) {
  await build.buildPreview();
}

if (process.argv.includes("build:editor-tools")) {
  await build.buildEditorTools();
}

if (process.argv.includes("build:vscode:web")) {
  await build.buildTinymistVscodeWeb();
}

if (process.argv.includes("build:vscode:system")) {
  await build.buildTinymistVscodeSystem();
}

if (process.argv.includes("build:lsp:debug")) {
  await build.buildDebugLspBinary();
}

if (process.argv.includes("prelaunch:vscode")) {
  await build.prelaunchVscode();
}

if (process.argv.includes("build:web:base")) {
  await build.buildWebLspBinaryBase();
}

if (process.argv.includes("build:web")) {
  await build.buildTinymistVscodeWeb();
}

// build:editor-tools
