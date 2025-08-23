import { genVscodeExt } from "./build-l10n.mjs";

import { spawn } from "child_process";
import * as fs from "fs/promises";
import * as path from "path";
import { rimraf } from "rimraf";

import {
  generate as generateTextmate,
  install as installTextmate,
} from "../syntaxes/textmate/main.ts";

/**
 * Spawn a command and return a promise that resolves to the code. The id is used to identify the command in the output and prepended to each line.
 * The line will be buffered and printed to avoid line crossing.
 */
export function spawnAsync(id, cmd, options = { cwd }) {
  return new Promise((resolve, reject) => {
    options.shell = true;
    options.stdio = "pipe";
    const child = spawn(cmd, options);

    const linePrinter = (stream, outStream) => {
      let out = "";
      stream.on("data", (data) => {
        out += data;
        const lines = out.split("\n");
        while (lines.length > 1) {
          const line = lines.shift();
          outStream.write(`[${id}] ${line}\n`);
        }
        out = lines.join("\n");
      });
      stream.on("end", () => {
        if (out) {
          outStream.write(`[${id}] ${out}\n`);
        }
      });
    };

    linePrinter(child.stdout, process.stdout);
    linePrinter(child.stderr, process.stderr);

    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`Command ${cmd} failed with code ${code}`));
      }
      resolve(code);
    });
  });
}

// # "extract:l10n:ts": "cargo run --release --bin tinymist-l10n -- --kind ts --dir ./editors/vscode --output ./locales/tinymist-vscode-rt.toml",
// # "extract:l10n:rs": "cargo run --release --bin tinymist-l10n -- --kind rs --dir ./crates --output ./locales/tinymist-rt.toml &&
// #  rimraf ./crates/tinymist-assets/src/tinymist-rt.toml && cpr ./locales/tinymist-rt.toml ./crates/tinymist-assets/src/tinymist-rt.toml",

const cwd = path.resolve(import.meta.dirname, "..");
const vscodeDir = path.resolve(cwd, "editors/vscode");
const editorToolsDir = path.resolve(cwd, "editors/editor-tools");

export async function extractL10nTs() {
  await spawnAsync(
    "extract:l10n:ts",
    "cargo run --release --bin tinymist-l10n -- --kind ts --dir ./editors/vscode --output ./locales/tinymist-vscode-rt.toml",
  );
}

export async function extractL10nRs() {
  await spawnAsync(
    "extract:l10n:rs",
    "cargo run --release --bin tinymist-l10n -- --kind rs --dir ./crates --output ./locales/tinymist-rt.toml",
  );
  await fs.copyFile(
    path.resolve(cwd, "locales/tinymist-rt.toml"),
    path.resolve(cwd, "crates/tinymist-assets/src/tinymist-rt.toml"),
  );
}

export async function buildL10n() {
  await Promise.all([extractL10nTs(), extractL10nRs()]);
  await genVscodeExt();
}

export async function buildSyntax() {
  await generateTextmate();
  await installTextmate();
}

// cd tools/typst-preview-frontend && yarn run build && rimraf ../../crates/tinymist-assets/src/typst-preview.html && cpr ./dist/index.html ../../crates/tinymist-assets/src/typst-preview.html
export async function buildPreview() {
  await Promise.all([
    spawnAsync("build:preview:tsc", "cd tools/typst-preview-frontend && npx tsc"),
    spawnAsync("build:preview:vite", "cd tools/typst-preview-frontend && npx vite build"),
  ]);

  // rimraf ../../crates/tinymist-assets/src/typst-preview.html && cpr ./dist/index.html ../../crates/tinymist-assets/src/typst-preview.html
  await fs.copyFile(
    path.resolve(cwd, "tools/typst-preview-frontend/dist/index.html"),
    path.resolve(cwd, "crates/tinymist-assets/src/typst-preview.html"),
  );
}

export async function buildEditorTools() {
  // tsc && vite build -- --component=symbol-view && vite build --
  await spawnAsync("build:editor-tools:tsc", "cd tools/editor-tools && npx tsc");
  await spawnAsync(
    "build:editor-tools:vite",
    "cd tools/editor-tools && npx vite build -- --component=symbol-view",
  );
  await spawnAsync("build:editor-tools:vite", "cd tools/editor-tools && npx vite build");
}

export async function installEditorTools() {
  await rimraf(path.join(vscodeDir, "out/editor-tools/"));
  await fs.mkdir(path.join(vscodeDir, "out/editor-tools/"), { recursive: true });
  await fs.copyFile(path.join(editorToolsDir, "dist"), path.join(vscodeDir, "out/editor-tools/"));
}

export async function checkVersion() {
  const cargoToml = await fs.readFile(path.resolve(cwd, "Cargo.toml"), "utf8");
  const cargoVersion = cargoToml.match(/version = "(.*?)"/)[1];
  const pkgVersion = JSON.parse(
    await fs.readFile(path.resolve(vscodeDir, "package.json"), "utf8"),
  ).version;

  if (cargoVersion !== pkgVersion) {
    throw new Error(
      `Version mismatch: ${cargoVersion} (in Cargo.toml) !== ${pkgVersion} (in package.json)`,
    );
  }
}

export async function buildTinymistVscodeWebBase() {
  await spawnAsync("vscode:web", "cd editors/vscode && node esbuild.web.mjs");
}

export async function buildTinymistVscodeWeb() {
  await Promise.all([
    checkVersion(),
    buildSyntax(),
    buildL10n(),
    buildEditorTools(),
    buildWebLspBinary().then(buildTinymistVscodeWebBase),
  ]);
}

export async function buildTinymistVscodeSystemBase() {
  await spawnAsync("vscode:system", "cd editors/vscode && node esbuild.system.mjs");
}

export async function buildTinymistVscodeSystem() {
  await Promise.all([
    checkVersion(),
    buildSyntax(),
    buildL10n(),
    buildEditorTools(),
    buildTinymistVscodeSystemBase(),
  ]);
}

//
// "dependsOn": [
//   "Build Debug LSP Binary",
//   "Copy Debug LSP Binary to VS Code Extension",
//   "Copy Debug LSP Debug Info to VS Code Extension",
//   "Compile Typst Preview Extension",
//   "Copy Debug LSP Binary to Typst Preview Extension"
// ],

export async function buildDebugLspBinary() {
  await spawnAsync("lsp:debug", "cargo build -p tinymist-cli --color=always", {
    env: {
      ...process.env,
      FORCE_COLOR: "1",
    },
  });

  const binName = process.platform === "win32" ? "tinymist.exe" : "tinymist";

  await Promise.all([
    fs.copyFile(
      path.resolve(cwd, `target/debug/${binName}`),
      path.resolve(vscodeDir, `out/${binName}`),
    ),
    process.platform === "win32"
      ? [
          fs.copyFile(
            path.resolve(cwd, `target/debug/tinymist.pdb`),
            path.resolve(vscodeDir, `out/tinymist.pdb`),
          ),
        ]
      : [],
  ]);
}

export async function prelaunchVscode() {
  await Promise.all([buildTinymistVscodeSystem(), buildDebugLspBinary()]);
}

export async function buildWebLspBinaryBase() {
  await spawnAsync(
    "lsp:web",
    "cd crates/tinymist && wasm-pack build --target web -- --no-default-features --features web,no-content-hint",
  );
}

export async function buildWebLspBinary() {
  await buildWebLspBinaryBase();
  await fs.copyFile(
    path.resolve(cwd, "crates/tinymist/pkg/tinymist_bg.wasm"),
    path.resolve(vscodeDir, "out/tinymist_bg.wasm"),
  );
}
