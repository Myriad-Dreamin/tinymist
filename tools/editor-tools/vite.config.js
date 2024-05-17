import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [viteSingleFile()],
  assetsInclude: ["**/*.onnx"],
  build: {
    minify: false,
    rollupOptions: {
      output: {
        assetFileNames: `typst-webview-assets/[name]-[hash][extname]`,
        chunkFileNames: "typst-webview-assets/[name]-[hash].js",
        entryFileNames: "typst-webview-assets/[name]-[hash].js",
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      loader: {
        ".onnx": "dataurl",
      },
    }
  }
});
