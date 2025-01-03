import { build } from "esbuild";

build({
  entryPoints: ["./src/extension.ts"],
  bundle: true,
  outfile: "./out/extension.js",
  external: ["vscode"],
  format: "cjs",
  platform: "node",
}).catch(() => process.exit(1));
