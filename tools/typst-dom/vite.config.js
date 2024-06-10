import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [viteSingleFile()],
  build: { minify: true, lib: { entry: "src/index.mts", name: "typst-dom" } },
});
