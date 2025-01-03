import tinymist_init from "../pkg/tinymist_core.js";
import * as tinymist from "../pkg/tinymist_core.js";
import fs from "fs";

const wasmData = fs.readFileSync("pkg/tinymist_core_bg.wasm");

async function main() {
  await tinymist_init({
    module_or_path: new Uint8Array(wasmData),
  });
  console.log(tinymist.version());
}

main().catch(console.error);
