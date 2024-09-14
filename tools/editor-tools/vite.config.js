import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// /src/main.ts

const compPrefix = '--component=';
const componentArgs = process.argv.find(arg => arg.startsWith(compPrefix));
let output = 'dist/default';
if (componentArgs) {
  const component = componentArgs.substring(compPrefix.length);
  process.env.VITE_ENTRY = `/src/main.${component}.ts`;
  output = `dist/${component}`;
} else {
  process.env.VITE_ENTRY = '/src/main.ts';
}

export default defineConfig({
  plugins: [viteSingleFile()],
  assetsInclude: ["**/*.onnx"],
  build: {
    minify: false,
    outDir: output
  },
  optimizeDeps: {
    esbuildOptions: {
      loader: {
        ".onnx": "dataurl",
      },
    }
  }
});
