import { generate, install } from "../main";

const isCompile = process.argv.includes("--compile");
const isInstall = process.argv.includes("--install");

if (isCompile) {
  await generate();
}
if (isInstall) {
  await install();
}
