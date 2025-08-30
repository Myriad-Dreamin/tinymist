import { build } from "esbuild";
import { polyfillNode } from "esbuild-plugin-polyfill-node";

import * as path from "path";
import * as fs from "fs";

let wasmPlugin = {
  name: "wasm",
  setup(build) {
    // Resolve ".wasm" files to a path with a namespace
    build.onResolve({ filter: /\.wasm$/ }, (args) => {
      if (args.resolveDir === "") {
        return; // Ignore unresolvable paths
      }
      return {
        path: path.isAbsolute(args.path) ? args.path : path.join(args.resolveDir, args.path),
        namespace: "wasm-binary",
      };
    });

    // Virtual modules in the "wasm-binary" namespace contain the
    // actual bytes of the WebAssembly file. This uses esbuild's
    // built-in "binary" loader instead of manually embedding the
    // binary data inside JavaScript code ourselves.
    build.onLoad({ filter: /.*/, namespace: "wasm-binary" }, async (args) => ({
      contents: await fs.promises.readFile(args.path),
      loader: "binary",
    }));
  },
};

build({
  entryPoints: ["./src/web/server.ts"],
  bundle: true,
  outfile: "./out/web-server.js",
  external: ["vscode"],
  format: "cjs",
  target: ["es2020", "chrome61", "edge18", "firefox60"],
  // Node.js global to browser globalThis
  define: {
    global: "globalThis",
  },
  plugins: [
    wasmPlugin,
    polyfillNode({
      polyfills: {
        crypto: "empty",
      },
      // Options (optional)
    }),
  ],
}).catch(() => process.exit(1));
