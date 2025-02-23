import { build } from "esbuild";
import * as fs from "fs";

if (!fs.existsSync("./out/extension.web.js")) {
  fs.mkdirSync("./out", { recursive: true });
  fs.writeFileSync("./out/extension.web.js", "");
}

build({
  entryPoints: ["./src/extension.mts", "./src/server.mts"],
  bundle: true,
  outdir: "./out",
  external: ["vscode"],
  format: "cjs",
  platform: "node",
}).catch(() => process.exit(1));
