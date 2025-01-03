import { build } from "esbuild";
import { polyfillNode } from "esbuild-plugin-polyfill-node";

build({
  entryPoints: ["./src/extension.web.ts"],
  bundle: true,
  outfile: "./out/extension.web.js",
  external: ["vscode"],
  format: "cjs",
  target: ["es2020", "chrome61", "edge18", "firefox60"],
  // Node.js global to browser globalThis
  define: {
    global: 'globalThis'
  },
  plugins: [
    polyfillNode({
      polyfills: {
        crypto: "empty",
      },
      // Options (optional)
    }),
  ],
}).catch(() => process.exit(1));
