import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  root: ".",
  base: "./",
  build: {
    outDir: "../../contrib/typst-preview-ng/editors/vscode/previewer",
    emptyOutDir: true,
    minify: false,
    sourcemap: true,
    assetsDir: "typst-webview-assets",
    rollupOptions: {
      input: resolve(import.meta.dirname, "index.html"),
      output: {
        assetFileNames: "typst-webview-assets/[name]-[hash][extname]",
        chunkFileNames: "typst-webview-assets/[name]-[hash].js",
        entryFileNames: "typst-webview-assets/[name]-[hash].js",
      },
    },
  },
  worker: {
    format: "es",
  },
});
