import { build } from "esbuild";
import * as fs from "fs";

if (!fs.existsSync("./out/extension.web.js")) {
  fs.mkdirSync("./out", { recursive: true });
  fs.writeFileSync("./out/extension.web.js", "");
}

build({
  entryPoints: ["./src/extension.ts"],
  bundle: true,
  outfile: "./out/extension.js",
  external: ["vscode"],
  format: "cjs",
  platform: "node",
}).catch(() => process.exit(1));
